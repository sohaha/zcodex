use crate::compression::ExplicitFallbackReason;
use sha1::Digest;
use sha1::Sha1;
use std::cmp::Reverse;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub(crate) struct NearDuplicateConfig {
    pub max_hamming_distance: u32,
    pub max_candidate_count: usize,
    pub min_similarity_ratio: f64,
    pub conflict_similarity_margin: f64,
    pub max_lcs_cells: usize,
    pub max_diff_lines: usize,
}

impl Default for NearDuplicateConfig {
    fn default() -> Self {
        Self {
            max_hamming_distance: 16,
            max_candidate_count: 4,
            min_similarity_ratio: 0.55,
            conflict_similarity_margin: 0.03,
            max_lcs_cells: 200_000,
            max_diff_lines: 48,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct NearDuplicateCandidate<'a> {
    pub fingerprint: &'a str,
    pub source_name: &'a str,
    pub snapshot: &'a str,
    pub output: &'a str,
    pub simhash: u64,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NearDuplicateOutcome {
    NoMatch,
    Diff(String),
    Fallback(ExplicitFallbackReason),
}

pub(crate) fn simhash(content: &str) -> u64 {
    let mut frequencies: HashMap<String, i32> = HashMap::new();
    for token in content
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .filter(|token| !token.is_empty())
    {
        *frequencies.entry(token.to_ascii_lowercase()).or_default() += 1;
    }

    if frequencies.is_empty() {
        for line in content.lines().filter(|line| !line.trim().is_empty()) {
            *frequencies.entry(line.trim().to_string()).or_default() += 1;
        }
    }

    if frequencies.is_empty() {
        return 0;
    }

    let mut bit_scores = [0_i32; 64];
    for (token, weight) in frequencies {
        let hash = hash_token(&token);
        for (bit, score) in bit_scores.iter_mut().enumerate() {
            if hash & (1_u64 << bit) == 0 {
                *score -= weight;
            } else {
                *score += weight;
            }
        }
    }

    let mut value = 0_u64;
    for (bit, score) in bit_scores.iter().enumerate() {
        if *score >= 0 {
            value |= 1_u64 << bit;
        }
    }
    value
}

pub(crate) fn analyze_near_duplicate(
    current_source: &str,
    current_output: &str,
    current_fingerprint: &str,
    current_simhash: u64,
    candidates: &[NearDuplicateCandidate<'_>],
    config: NearDuplicateConfig,
) -> NearDuplicateOutcome {
    let mut threshold_candidates: Vec<(u32, &NearDuplicateCandidate<'_>)> = candidates
        .iter()
        .map(|candidate| {
            (
                hamming_distance(current_simhash, candidate.simhash),
                candidate,
            )
        })
        .filter(|(distance, _)| *distance <= config.max_hamming_distance)
        .collect();

    if threshold_candidates.is_empty() {
        return NearDuplicateOutcome::NoMatch;
    }

    threshold_candidates
        .sort_by_key(|(distance, candidate)| (*distance, Reverse(candidate.created_at)));

    let mut matches = Vec::new();
    let mut saw_snapshot_missing = false;
    let mut saw_low_confidence = false;
    let mut saw_diff_unreadable = false;
    let mut saw_strategy_unavailable = false;

    for (distance, candidate) in threshold_candidates
        .into_iter()
        .take(config.max_candidate_count)
    {
        match evaluate_candidate(
            current_source,
            current_output,
            current_fingerprint,
            candidate,
            distance,
            config,
        ) {
            CandidateEvaluation::Match(matched) => matches.push(matched),
            CandidateEvaluation::LowConfidence => saw_low_confidence = true,
            CandidateEvaluation::SnapshotUnavailable => saw_snapshot_missing = true,
            CandidateEvaluation::DiffUnreadable => saw_diff_unreadable = true,
            CandidateEvaluation::StrategyUnavailable => saw_strategy_unavailable = true,
        }
    }

    if matches.is_empty() {
        if saw_snapshot_missing {
            return NearDuplicateOutcome::Fallback(
                ExplicitFallbackReason::NearDuplicateSnapshotUnavailable,
            );
        }
        if saw_diff_unreadable {
            return NearDuplicateOutcome::Fallback(
                ExplicitFallbackReason::NearDuplicateDiffUnreadable,
            );
        }
        if saw_strategy_unavailable {
            return NearDuplicateOutcome::Fallback(ExplicitFallbackReason::StrategyUnavailable);
        }
        if saw_low_confidence {
            return NearDuplicateOutcome::Fallback(
                ExplicitFallbackReason::NearDuplicateLowConfidence,
            );
        }
        return NearDuplicateOutcome::NoMatch;
    }

    matches.sort_by(|left, right| {
        right
            .same_source
            .cmp(&left.same_source)
            .then_with(|| right.similarity.total_cmp(&left.similarity))
            .then_with(|| left.hamming_distance.cmp(&right.hamming_distance))
            .then_with(|| right.created_at.cmp(&left.created_at))
    });

    if matches.len() > 1 {
        let delta = (matches[0].similarity - matches[1].similarity).abs();
        if delta <= config.conflict_similarity_margin {
            return NearDuplicateOutcome::Fallback(
                ExplicitFallbackReason::NearDuplicateCandidateConflict,
            );
        }
    }

    NearDuplicateOutcome::Diff(matches.remove(0).rendered_diff)
}

fn evaluate_candidate(
    current_source: &str,
    current_output: &str,
    current_fingerprint: &str,
    candidate: &NearDuplicateCandidate<'_>,
    hamming_distance: u32,
    config: NearDuplicateConfig,
) -> CandidateEvaluation {
    if candidate.snapshot.trim().is_empty() || candidate.output.trim().is_empty() {
        return CandidateEvaluation::SnapshotUnavailable;
    }

    let Some(diff_plan) = build_diff_plan(candidate.output, current_output, config.max_lcs_cells)
    else {
        return CandidateEvaluation::StrategyUnavailable;
    };

    let denominator = diff_plan
        .previous_line_count
        .max(diff_plan.current_line_count)
        .max(1);
    let similarity = diff_plan.lcs_length as f64 / denominator as f64;
    if similarity < config.min_similarity_ratio {
        return CandidateEvaluation::LowConfidence;
    }

    if diff_plan.changed_line_count > config.max_diff_lines {
        return CandidateEvaluation::DiffUnreadable;
    }

    CandidateEvaluation::Match(CandidateMatch {
        same_source: candidate.source_name == current_source,
        similarity,
        hamming_distance,
        created_at: candidate.created_at,
        rendered_diff: render_diff(
            current_source,
            current_fingerprint,
            candidate,
            similarity,
            diff_plan.operations,
        ),
    })
}

fn hash_token(token: &str) -> u64 {
    let mut hasher = Sha1::new();
    hasher.update(token.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    u64::from_be_bytes(bytes)
}

fn hamming_distance(left: u64, right: u64) -> u32 {
    (left ^ right).count_ones()
}

#[derive(Debug)]
struct CandidateMatch {
    same_source: bool,
    similarity: f64,
    hamming_distance: u32,
    created_at: i64,
    rendered_diff: String,
}

#[derive(Debug)]
enum CandidateEvaluation {
    Match(CandidateMatch),
    LowConfidence,
    SnapshotUnavailable,
    DiffUnreadable,
    StrategyUnavailable,
}

#[derive(Debug)]
struct DiffPlan<'a, 'b> {
    operations: Vec<LineOperation<'a, 'b>>,
    previous_line_count: usize,
    current_line_count: usize,
    lcs_length: usize,
    changed_line_count: usize,
}

#[derive(Debug)]
enum LineOperation<'a, 'b> {
    Equal,
    Delete(&'a str),
    Insert(&'b str),
}

fn build_diff_plan<'a, 'b>(
    previous: &'a str,
    current: &'b str,
    max_lcs_cells: usize,
) -> Option<DiffPlan<'a, 'b>> {
    let previous_lines: Vec<&str> = previous.lines().collect();
    let current_lines: Vec<&str> = current.lines().collect();
    let cell_count = previous_lines.len().saturating_mul(current_lines.len());
    if cell_count > max_lcs_cells {
        return None;
    }

    let table = build_lcs_table(&previous_lines, &current_lines);
    let (operations, lcs_length) = build_line_operations(&previous_lines, &current_lines, &table);
    let changed_line_count = operations
        .iter()
        .filter(|operation| !matches!(operation, LineOperation::Equal))
        .count();

    Some(DiffPlan {
        operations,
        previous_line_count: previous_lines.len(),
        current_line_count: current_lines.len(),
        lcs_length,
        changed_line_count,
    })
}

fn build_lcs_table(previous: &[&str], current: &[&str]) -> Vec<Vec<usize>> {
    let mut table = vec![vec![0; current.len() + 1]; previous.len() + 1];
    for previous_index in 0..previous.len() {
        for current_index in 0..current.len() {
            table[previous_index + 1][current_index + 1] =
                if previous[previous_index] == current[current_index] {
                    table[previous_index][current_index] + 1
                } else {
                    table[previous_index][current_index + 1]
                        .max(table[previous_index + 1][current_index])
                };
        }
    }
    table
}

fn build_line_operations<'a, 'b>(
    previous: &[&'a str],
    current: &[&'b str],
    table: &[Vec<usize>],
) -> (Vec<LineOperation<'a, 'b>>, usize) {
    let mut previous_index = previous.len();
    let mut current_index = current.len();
    let mut operations = Vec::new();
    let mut lcs_length = 0;

    while previous_index > 0 && current_index > 0 {
        if previous[previous_index - 1] == current[current_index - 1] {
            operations.push(LineOperation::Equal);
            previous_index -= 1;
            current_index -= 1;
            lcs_length += 1;
        } else if table[previous_index - 1][current_index]
            >= table[previous_index][current_index - 1]
        {
            operations.push(LineOperation::Delete(previous[previous_index - 1]));
            previous_index -= 1;
        } else {
            operations.push(LineOperation::Insert(current[current_index - 1]));
            current_index -= 1;
        }
    }

    while previous_index > 0 {
        operations.push(LineOperation::Delete(previous[previous_index - 1]));
        previous_index -= 1;
    }

    while current_index > 0 {
        operations.push(LineOperation::Insert(current[current_index - 1]));
        current_index -= 1;
    }

    operations.reverse();
    (operations, lcs_length)
}

fn render_diff(
    current_source: &str,
    current_fingerprint: &str,
    candidate: &NearDuplicateCandidate<'_>,
    similarity: f64,
    operations: Vec<LineOperation<'_, '_>>,
) -> String {
    let short = &current_fingerprint[..8];
    let base_short = &candidate.fingerprint[..candidate.fingerprint.len().min(8)];
    let source_context = if candidate.source_name == current_source {
        format!("同一会话内已输出该内容的近似版本（基线 {base_short}）")
    } else {
        format!(
            "与 {} 的已输出内容高度相似（基线 {base_short}）",
            candidate.source_name
        )
    };

    let mut rendered = format!(
        "[ztok diff {short}] {source_context}，仅输出变化部分（相似度 {:.0}%）\n",
        similarity * 100.0
    );

    let mut unchanged_run = 0_usize;
    for operation in operations {
        match operation {
            LineOperation::Equal => unchanged_run += 1,
            LineOperation::Delete(line) => {
                flush_unchanged_run(&mut rendered, &mut unchanged_run);
                rendered.push_str(&format!("- {line}\n"));
            }
            LineOperation::Insert(line) => {
                flush_unchanged_run(&mut rendered, &mut unchanged_run);
                rendered.push_str(&format!("+ {line}\n"));
            }
        }
    }
    flush_unchanged_run(&mut rendered, &mut unchanged_run);
    rendered.trim_end().to_string()
}

fn flush_unchanged_run(rendered: &mut String, unchanged_run: &mut usize) {
    if *unchanged_run == 0 {
        return;
    }
    rendered.push_str(&format!("… {} 行未变 …\n", *unchanged_run));
    *unchanged_run = 0;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate<'a>(
        fingerprint: &'a str,
        source_name: &'a str,
        snapshot: &'a str,
        output: &'a str,
    ) -> NearDuplicateCandidate<'a> {
        NearDuplicateCandidate {
            fingerprint,
            source_name,
            snapshot,
            output,
            simhash: simhash(snapshot),
            created_at: 1,
        }
    }

    #[test]
    fn simhash_stays_close_for_small_edits() {
        let left = simhash("fn main() {\n    let answer = 41;\n}\n");
        let right = simhash("fn main() {\n    let answer = 42;\n}\n");
        assert!(
            hamming_distance(left, right) <= NearDuplicateConfig::default().max_hamming_distance
        );
    }

    #[test]
    fn unreadable_diff_falls_back_explicitly() {
        let config = NearDuplicateConfig {
            max_hamming_distance: 64,
            max_diff_lines: 1,
            ..NearDuplicateConfig::default()
        };
        let outcome = analyze_near_duplicate(
            "sample.rs",
            "line1\nline2-new\nline3-new\nline4\nline5\n",
            "abcdef1234567890",
            simhash("line1\nline2-new\nline3-new\nline4\nline5\n"),
            &[candidate(
                "feedfacecafebeef",
                "sample.rs",
                "line1\nline2-old\nline3-old\nline4\nline5\n",
                "line1\nline2-old\nline3-old\nline4\nline5\n",
            )],
            config,
        );

        assert_eq!(
            outcome,
            NearDuplicateOutcome::Fallback(ExplicitFallbackReason::NearDuplicateDiffUnreadable)
        );
    }
}
