use crate::tracking;
use crate::utils::resolved_command;
use anyhow::Context;
use anyhow::Result;
use std::ffi::OsString;

#[derive(Debug, Clone, Copy)]
pub enum ContainerCmd {
    DockerPs,
    DockerImages,
    DockerLogs,
    KubectlPods,
    KubectlServices,
    KubectlLogs,
}

pub fn run(cmd: ContainerCmd, args: &[String], verbose: u8) -> Result<()> {
    match cmd {
        ContainerCmd::DockerPs => docker_ps(verbose),
        ContainerCmd::DockerImages => docker_images(verbose),
        ContainerCmd::DockerLogs => docker_logs(args, verbose),
        ContainerCmd::KubectlPods => kubectl_pods(args, verbose),
        ContainerCmd::KubectlServices => kubectl_services(args, verbose),
        ContainerCmd::KubectlLogs => kubectl_logs(args, verbose),
    }
}

fn docker_ps(_verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let raw = resolved_command("docker")
        .args(["ps"])
        .output()
        .map(|o| crate::utils::decode_output(&o.stdout).to_string())
        .unwrap_or_default();

    let output = resolved_command("docker")
        .args([
            "ps",
            "--format",
            "{{.ID}}\t{{.Names}}\t{{.Status}}\t{{.Image}}\t{{.Ports}}",
        ])
        .output()
        .context("运行 docker ps 失败")?;

    if !output.status.success() {
        let stderr = crate::utils::decode_output(&output.stderr);
        eprint!("{stderr}");
        timer.track("docker ps", "rtk docker ps", &raw, &raw);
        std::process::exit(output.status.code().unwrap_or(1));
    }

    let stdout = crate::utils::decode_output(&output.stdout);
    let mut rtk = String::new();

    if stdout.trim().is_empty() {
        rtk.push_str("🐳 0 个容器");
        println!("{rtk}");
        timer.track("docker ps", "rtk docker ps", &raw, &rtk);
        return Ok(());
    }

    let count = stdout.lines().count();
    rtk.push_str(&format!("🐳 {count} 个容器：\n"));

    for line in stdout.lines().take(15) {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 4 {
            let id = &parts[0][..12.min(parts[0].len())];
            let name = parts[1];
            let short_image = parts
                .get(3)
                .unwrap_or(&"")
                .split('/')
                .next_back()
                .unwrap_or("");
            let ports = compact_ports(parts.get(4).unwrap_or(&""));
            if ports == "-" {
                rtk.push_str(&format!("  {id} {name} ({short_image})\n"));
            } else {
                rtk.push_str(&format!("  {id} {name} ({short_image}) [{ports}]\n"));
            }
        }
    }
    if count > 15 {
        rtk.push_str(&format!("  ... +{} 个", count - 15));
    }

    print!("{rtk}");
    timer.track("docker ps", "rtk docker ps", &raw, &rtk);
    Ok(())
}

fn docker_images(_verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let raw = resolved_command("docker")
        .args(["images"])
        .output()
        .map(|o| crate::utils::decode_output(&o.stdout).to_string())
        .unwrap_or_default();

    let output = resolved_command("docker")
        .args(["images", "--format", "{{.Repository}}:{{.Tag}}\t{{.Size}}"])
        .output()
        .context("运行 docker images 失败")?;

    if !output.status.success() {
        let stderr = crate::utils::decode_output(&output.stderr);
        eprint!("{stderr}");
        timer.track("docker images", "rtk docker images", &raw, &raw);
        std::process::exit(output.status.code().unwrap_or(1));
    }

    let stdout = crate::utils::decode_output(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    let mut rtk = String::new();

    if lines.is_empty() {
        rtk.push_str("🐳 0 个镜像");
        println!("{rtk}");
        timer.track("docker images", "rtk docker images", &raw, &rtk);
        return Ok(());
    }

    let mut total_size_mb: f64 = 0.0;
    for line in &lines {
        let parts: Vec<&str> = line.split('\t').collect();
        if let Some(size_str) = parts.get(1) {
            if size_str.contains("GB") {
                if let Ok(n) = size_str.replace("GB", "").trim().parse::<f64>() {
                    total_size_mb += n * 1024.0;
                }
            } else if size_str.contains("MB")
                && let Ok(n) = size_str.replace("MB", "").trim().parse::<f64>()
            {
                total_size_mb += n;
            }
        }
    }

    let total_display = if total_size_mb > 1024.0 {
        format!("{:.1}GB", total_size_mb / 1024.0)
    } else {
        format!("{total_size_mb:.0}MB")
    };
    rtk.push_str(&format!("🐳 {} 个镜像（{}）\n", lines.len(), total_display));

    for line in lines.iter().take(15) {
        let parts: Vec<&str> = line.split('\t').collect();
        if !parts.is_empty() {
            let image = parts[0];
            let size = parts.get(1).unwrap_or(&"");
            let short = if image.len() > 40 {
                format!("...{}", &image[image.len() - 37..])
            } else {
                image.to_string()
            };
            rtk.push_str(&format!("  {short} [{size}]\n"));
        }
    }
    if lines.len() > 15 {
        rtk.push_str(&format!("  ... +{} 个", lines.len() - 15));
    }

    print!("{rtk}");
    timer.track("docker images", "rtk docker images", &raw, &rtk);
    Ok(())
}

fn docker_logs(args: &[String], _verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let container = args.first().map(std::string::String::as_str).unwrap_or("");
    if container.is_empty() {
        println!("用法：rtk docker logs <container>");
        return Ok(());
    }

    let output = resolved_command("docker")
        .args(["logs", "--tail", "100", container])
        .output()
        .context("运行 docker logs 失败")?;

    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let raw = format!("{stdout}\n{stderr}");

    let analyzed = crate::log_cmd::run_stdin_str(&raw);
    let rtk = format!("🐳 {container} 日志：\n{analyzed}");
    println!("{rtk}");
    timer.track(
        &format!("docker logs {container}"),
        "rtk docker logs",
        &raw,
        &rtk,
    );
    Ok(())
}

fn kubectl_pods(args: &[String], _verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut cmd = resolved_command("kubectl");
    cmd.args(["get", "pods", "-o", "json"]);
    for arg in args {
        cmd.arg(arg);
    }

    let output = cmd.output().context("运行 kubectl get pods 失败")?;
    let raw = crate::utils::decode_output(&output.stdout).to_string();
    let mut rtk = String::new();

    let json: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => {
            rtk.push_str("☸️  未找到 Pod");
            println!("{rtk}");
            timer.track("kubectl get pods", "rtk kubectl pods", &raw, &rtk);
            return Ok(());
        }
    };

    let Some(pods) = json["items"].as_array().filter(|a| !a.is_empty()) else {
        rtk.push_str("☸️  未找到 Pod");
        println!("{rtk}");
        timer.track("kubectl get pods", "rtk kubectl pods", &raw, &rtk);
        return Ok(());
    };
    let (mut running, mut pending, mut failed, mut restarts_total) = (0, 0, 0, 0i64);
    let mut issues: Vec<String> = Vec::new();

    for pod in pods {
        let ns = pod["metadata"]["namespace"].as_str().unwrap_or("-");
        let name = pod["metadata"]["name"].as_str().unwrap_or("-");
        let phase = pod["status"]["phase"].as_str().unwrap_or("未知");

        if let Some(containers) = pod["status"]["containerStatuses"].as_array() {
            for c in containers {
                restarts_total += c["restartCount"].as_i64().unwrap_or(0);
            }
        }

        match phase {
            "Running" => running += 1,
            "Pending" => {
                pending += 1;
                issues.push(format!("{ns}/{name} 等待"));
            }
            "Failed" | "Error" => {
                failed += 1;
                let phase_label = match phase {
                    "Failed" => "失败",
                    "Error" => "错误",
                    _ => phase,
                };
                issues.push(format!("{ns}/{name} {phase_label}"));
            }
            _ => {
                if let Some(containers) = pod["status"]["containerStatuses"].as_array() {
                    for c in containers {
                        if let Some(w) = c["state"]["waiting"]["reason"].as_str()
                            && (w.contains("CrashLoop") || w.contains("Error"))
                        {
                            failed += 1;
                            issues.push(format!("{ns}/{name} {w}"));
                        }
                    }
                }
            }
        }
    }

    let mut parts = Vec::new();
    if running > 0 {
        parts.push(format!("{running} ✓"));
    }
    if pending > 0 {
        parts.push(format!("{pending} 等待"));
    }
    if failed > 0 {
        parts.push(format!("{failed} ✗"));
    }
    if restarts_total > 0 {
        parts.push(format!("{restarts_total} 次重启"));
    }

    rtk.push_str(&format!(
        "☸️  {} 个 Pod：{}\n",
        pods.len(),
        parts.join(", ")
    ));
    if !issues.is_empty() {
        rtk.push_str("⚠️  问题：\n");
        for issue in issues.iter().take(10) {
            rtk.push_str(&format!("  {issue}\n"));
        }
        if issues.len() > 10 {
            rtk.push_str(&format!("  ... +{} 个", issues.len() - 10));
        }
    }

    print!("{rtk}");
    timer.track("kubectl get pods", "rtk kubectl pods", &raw, &rtk);
    Ok(())
}

fn kubectl_services(args: &[String], _verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut cmd = resolved_command("kubectl");
    cmd.args(["get", "services", "-o", "json"]);
    for arg in args {
        cmd.arg(arg);
    }

    let output = cmd.output().context("运行 kubectl get services 失败")?;
    let raw = crate::utils::decode_output(&output.stdout).to_string();
    let mut rtk = String::new();

    let json: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => {
            rtk.push_str("☸️  未找到 Service");
            println!("{rtk}");
            timer.track("kubectl get svc", "rtk kubectl svc", &raw, &rtk);
            return Ok(());
        }
    };

    let Some(services) = json["items"].as_array().filter(|a| !a.is_empty()) else {
        rtk.push_str("☸️  未找到 Service");
        println!("{rtk}");
        timer.track("kubectl get svc", "rtk kubectl svc", &raw, &rtk);
        return Ok(());
    };
    rtk.push_str(&format!("☸️  {} 个 Service：\n", services.len()));

    for svc in services.iter().take(15) {
        let ns = svc["metadata"]["namespace"].as_str().unwrap_or("-");
        let name = svc["metadata"]["name"].as_str().unwrap_or("-");
        let svc_type = svc["spec"]["type"].as_str().unwrap_or("-");
        let ports: Vec<String> = svc["spec"]["ports"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|p| {
                        let port = p["port"].as_i64().unwrap_or(0);
                        let target = p["targetPort"]
                            .as_i64()
                            .or_else(|| p["targetPort"].as_str().and_then(|s| s.parse().ok()))
                            .unwrap_or(port);
                        if port == target {
                            format!("{port}")
                        } else {
                            format!("{port}→{target}")
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        rtk.push_str(&format!(
            "  {}/{} {} [{}]\n",
            ns,
            name,
            svc_type,
            ports.join(",")
        ));
    }
    if services.len() > 15 {
        rtk.push_str(&format!("  ... +{} 个", services.len() - 15));
    }

    print!("{rtk}");
    timer.track("kubectl get svc", "rtk kubectl svc", &raw, &rtk);
    Ok(())
}

fn kubectl_logs(args: &[String], _verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let pod = args.first().map(std::string::String::as_str).unwrap_or("");
    if pod.is_empty() {
        println!("用法：rtk kubectl logs <pod>");
        return Ok(());
    }

    let mut cmd = resolved_command("kubectl");
    cmd.args(["logs", "--tail", "100", pod]);
    for arg in args.iter().skip(1) {
        cmd.arg(arg);
    }

    let output = cmd.output().context("运行 kubectl logs 失败")?;
    let raw = crate::utils::decode_output(&output.stdout).to_string();
    let analyzed = crate::log_cmd::run_stdin_str(&raw);
    let rtk = format!("☸️  {pod} 日志：\n{analyzed}");
    println!("{rtk}");
    timer.track(
        &format!("kubectl logs {pod}"),
        "rtk kubectl logs",
        &raw,
        &rtk,
    );
    Ok(())
}

/// 将 `docker compose ps --format` 的输出压缩成紧凑格式。
/// 期望输入为制表符分隔的行：`Name\tImage\tStatus\tPorts`
/// （无表头，因为 `--format` 输出本身不带表头）
pub fn format_compose_ps(raw: &str) -> String {
    let lines: Vec<&str> = raw.lines().filter(|l| !l.trim().is_empty()).collect();

    if lines.is_empty() {
        return "🐳 0 个 Compose 服务".to_string();
    }

    let mut result = format!("🐳 {} 个 Compose 服务：\n", lines.len());

    for line in lines.iter().take(20) {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 4 {
            let name = parts[0];
            let image = parts[1];
            let status = parts[2];
            let ports = parts[3];

            let short_image = image.split('/').next_back().unwrap_or(image);

            let port_str = if ports.trim().is_empty() {
                String::new()
            } else {
                let compact = compact_ports(ports.trim());
                if compact == "-" {
                    String::new()
                } else {
                    format!(" [{compact}]")
                }
            };

            result.push_str(&format!("  {name} ({short_image}) {status}{port_str}\n"));
        }
    }
    if lines.len() > 20 {
        result.push_str(&format!("  ... +{} 个\n", lines.len() - 20));
    }

    result.trim_end().to_string()
}

/// 将 `docker compose logs` 的输出压缩成紧凑格式
pub fn format_compose_logs(raw: &str) -> String {
    if raw.trim().is_empty() {
        return "🐳 无日志".to_string();
    }

    // `docker compose logs` 会为每一行添加 `"service-N  | "` 前缀
    // 复用现有的日志去重引擎
    let analyzed = crate::log_cmd::run_stdin_str(raw);
    format!("🐳 Compose 日志：\n{analyzed}")
}

/// 将 `docker compose build` 的输出压缩为摘要
pub fn format_compose_build(raw: &str) -> String {
    if raw.trim().is_empty() {
        return "🐳 构建：无输出".to_string();
    }

    let mut result = String::new();

    // 提取摘要行，例如 "[+] Building 12.3s (8/8) FINISHED"
    for line in raw.lines() {
        if line.contains("Building") && line.contains("FINISHED") {
            result.push_str(&format!("🐳 {}\n", line.trim()));
            break;
        }
    }

    if result.is_empty() {
        // 没找到 FINISHED 行，可能仍在构建中或已报错
        if let Some(line) = raw.lines().find(|l| l.contains("Building")) {
            result.push_str(&format!("🐳 {}\n", line.trim()));
        } else {
            result.push_str("🐳 构建：\n");
        }
    }

    // 从类似 "[web 1/4]" 的构建步骤里提取唯一服务名
    let mut services: Vec<String> = Vec::new();
    // `find('[')` 返回字节偏移，因此这里全程使用字节切片
    // '[' 和 ']' 都是单字节 ASCII，字节运算是安全的
    for line in raw.lines() {
        if let Some(start) = line.find('[')
            && let Some(end) = line[start + 1..].find(']')
        {
            let bracket = &line[start + 1..start + 1 + end];
            let svc = bracket.split_whitespace().next().unwrap_or("");
            if !svc.is_empty() && svc != "+" && !services.contains(&svc.to_string()) {
                services.push(svc.to_string());
            }
        }
    }

    if !services.is_empty() {
        result.push_str(&format!("  服务：{}\n", services.join(", ")));
    }

    // 统计构建步骤数（以 " => " 开头的行）
    let step_count = raw
        .lines()
        .filter(|l| l.trim_start().starts_with("=> "))
        .count();
    if step_count > 0 {
        result.push_str(&format!("  步骤：{step_count}"));
    }

    result.trim_end().to_string()
}

fn compact_ports(ports: &str) -> String {
    if ports.is_empty() {
        return "-".to_string();
    }

    // 仅提取端口号
    let port_nums: Vec<&str> = ports
        .split(',')
        .filter_map(|p| p.split("->").next().and_then(|s| s.split(':').next_back()))
        .collect();

    if port_nums.len() <= 3 {
        port_nums.join(", ")
    } else {
        format!(
            "{}, ... +{}",
            port_nums[..2].join(", "),
            port_nums.len() - 2
        )
    }
}

/// 对不支持的 docker 子命令直接透传执行
pub fn run_docker_passthrough(args: &[OsString], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    if verbose > 0 {
        eprintln!("docker 透传：{args:?}");
    }
    let status = resolved_command("docker")
        .args(args)
        .status()
        .context("运行 docker 失败")?;

    let args_str = tracking::args_display(args);
    timer.track_passthrough(
        &format!("docker {args_str}"),
        &format!("rtk docker {args_str} (passthrough)"),
    );

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

/// 以紧凑输出运行 `docker compose ps`
pub fn run_compose_ps(verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    // 原始输出用于 token 统计
    let raw_output = resolved_command("docker")
        .args(["compose", "ps"])
        .output()
        .context("运行 docker compose ps 失败")?;

    if !raw_output.status.success() {
        let stderr = crate::utils::decode_output(&raw_output.stderr);
        eprintln!("{stderr}");
        std::process::exit(raw_output.status.code().unwrap_or(1));
    }
    let raw = crate::utils::decode_output(&raw_output.stdout).to_string();

    // 结构化输出用于解析（与 docker_ps 使用相同模式）
    let output = resolved_command("docker")
        .args([
            "compose",
            "ps",
            "--format",
            "{{.Name}}\t{{.Image}}\t{{.Status}}\t{{.Ports}}",
        ])
        .output()
        .context("运行 docker compose ps --format 失败")?;

    if !output.status.success() {
        let stderr = crate::utils::decode_output(&output.stderr);
        eprintln!("{stderr}");
        std::process::exit(output.status.code().unwrap_or(1));
    }
    let structured = crate::utils::decode_output(&output.stdout).to_string();

    if verbose > 0 {
        eprintln!("原始 docker compose ps：\n{raw}");
    }

    let rtk = format_compose_ps(&structured);
    println!("{rtk}");
    timer.track("docker compose ps", "rtk docker compose ps", &raw, &rtk);
    Ok(())
}

/// 运行 `docker compose logs` 并去重
pub fn run_compose_logs(service: Option<&str>, verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut cmd = resolved_command("docker");
    cmd.args(["compose", "logs", "--tail", "100"]);
    if let Some(svc) = service {
        cmd.arg(svc);
    }

    let output = cmd.output().context("运行 docker compose logs 失败")?;

    if !output.status.success() {
        let stderr = crate::utils::decode_output(&output.stderr);
        eprintln!("{stderr}");
        std::process::exit(output.status.code().unwrap_or(1));
    }

    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let raw = format!("{stdout}\n{stderr}");

    if verbose > 0 {
        eprintln!("原始 docker compose logs：\n{raw}");
    }

    let rtk = format_compose_logs(&raw);
    println!("{rtk}");
    let svc_label = service.unwrap_or("all");
    timer.track(
        &format!("docker compose logs {svc_label}"),
        "rtk docker compose logs",
        &raw,
        &rtk,
    );
    Ok(())
}

/// 以摘要形式运行 `docker compose build`
pub fn run_compose_build(service: Option<&str>, verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut cmd = resolved_command("docker");
    cmd.args(["compose", "build"]);
    if let Some(svc) = service {
        cmd.arg(svc);
    }

    let output = cmd.output().context("运行 docker compose build 失败")?;

    if !output.status.success() {
        let stderr = crate::utils::decode_output(&output.stderr);
        eprintln!("{stderr}");
        std::process::exit(output.status.code().unwrap_or(1));
    }

    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let raw = format!("{stdout}\n{stderr}");

    if verbose > 0 {
        eprintln!("原始 docker compose build：\n{raw}");
    }

    let rtk = format_compose_build(&raw);
    println!("{rtk}");
    let svc_label = service.unwrap_or("all");
    timer.track(
        &format!("docker compose build {svc_label}"),
        "rtk docker compose build",
        &raw,
        &rtk,
    );
    Ok(())
}

/// 对不支持的 docker compose 子命令直接透传执行
pub fn run_compose_passthrough(args: &[OsString], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    if verbose > 0 {
        eprintln!("docker compose 透传：{args:?}");
    }
    let status = resolved_command("docker")
        .arg("compose")
        .args(args)
        .status()
        .context("运行 docker compose 失败")?;

    let args_str = tracking::args_display(args);
    timer.track_passthrough(
        &format!("docker compose {args_str}"),
        &format!("rtk docker compose {args_str} (passthrough)"),
    );

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

/// 对不支持的 kubectl 子命令直接透传执行
pub fn run_kubectl_passthrough(args: &[OsString], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    if verbose > 0 {
        eprintln!("kubectl 透传：{args:?}");
    }
    let status = resolved_command("kubectl")
        .args(args)
        .status()
        .context("运行 kubectl 失败")?;

    let args_str = tracking::args_display(args);
    timer.track_passthrough(
        &format!("kubectl {args_str}"),
        &format!("rtk kubectl {args_str} (passthrough)"),
    );

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── format_compose_ps ──────────────────────────────────

    #[test]
    fn test_format_compose_ps_basic() {
        // 制表符分隔的 --format 输出：Name\tImage\tStatus\tPorts
        let raw = "web-1\tnginx:latest\tUp 2 hours\t0.0.0.0:80->80/tcp\n\
                   api-1\tnode:20\tUp 2 hours\t0.0.0.0:3000->3000/tcp\n\
                   db-1\tpostgres:16\tUp 2 hours\t0.0.0.0:5432->5432/tcp";
        let out = format_compose_ps(raw);
        assert!(out.contains("3"), "应显示容器数量");
        assert!(out.contains("web"), "应显示服务名");
        assert!(out.contains("api"), "应显示服务名");
        assert!(out.contains("db"), "应显示服务名");
        assert!(out.contains("Up 2 hours"), "应显示状态");
        assert!(out.len() < raw.len(), "输出应短于原始内容");
    }

    #[test]
    fn test_format_compose_ps_empty() {
        let out = format_compose_ps("");
        assert!(out.contains("0"), "应显示零个容器");
    }

    #[test]
    fn test_format_compose_ps_whitespace_only() {
        let out = format_compose_ps("   \n  \n");
        assert!(out.contains("0"), "应显示零个容器");
    }

    #[test]
    fn test_format_compose_ps_exited_service() {
        // 制表符分隔的 --format 输出
        let raw = "worker-1\tpython:3.12\tExited (1) 2 minutes ago\t";
        let out = format_compose_ps(raw);
        assert!(out.contains("worker"), "应显示服务名");
        assert!(out.contains("Exited"), "应显示退出状态");
    }

    #[test]
    fn test_format_compose_ps_no_ports() {
        let raw = "redis-1\tredis:7\tUp 5 hours\t";
        let out = format_compose_ps(raw);
        assert!(out.contains("redis"), "应显示服务名");
        assert!(!out.contains("["), "端口为空时不应显示端口括号");
    }

    #[test]
    fn test_format_compose_ps_long_image_path() {
        let raw = "app-1\tghcr.io/myorg/myapp:latest\tUp 1 hour\t0.0.0.0:8080->8080/tcp";
        let out = format_compose_ps(raw);
        assert!(out.contains("myapp:latest"), "应将镜像缩短为最后一段");
        assert!(!out.contains("ghcr.io"), "不应显示完整镜像仓库路径");
    }

    // ── format_compose_logs ────────────────────────────────

    #[test]
    fn test_format_compose_logs_basic() {
        let raw = "\
web-1  | 192.168.1.1 - GET / 200
web-1  | 192.168.1.1 - GET /favicon.ico 404
api-1  | Server listening on port 3000
api-1  | Connected to database";
        let out = format_compose_logs(raw);
        assert!(out.contains("Compose 日志"), "应带有 Compose 日志标题");
    }

    #[test]
    fn test_format_compose_logs_empty() {
        let out = format_compose_logs("");
        assert!(out.contains("无日志"), "应提示无日志");
    }

    // ── format_compose_build ───────────────────────────────

    #[test]
    fn test_format_compose_build_basic() {
        let raw = "\
[+] Building 12.3s (8/8) FINISHED
 => [web internal] load build definition from Dockerfile           0.0s
 => [web internal] load metadata for docker.io/library/node:20     1.2s
 => [web 1/4] FROM docker.io/library/node:20@sha256:abc123         0.0s
 => [web 2/4] WORKDIR /app                                         0.1s
 => [web 3/4] COPY package*.json ./                                0.1s
 => [web 4/4] RUN npm install                                      8.5s
 => [web] exporting to image                                       2.3s
 => => naming to docker.io/library/myapp-web                       0.0s";
        let out = format_compose_build(raw);
        assert!(out.contains("12.3s"), "应显示总构建时间");
        assert!(out.contains("web"), "应显示服务名");
        assert!(out.len() < raw.len(), "输出应短于原始内容");
    }

    #[test]
    fn test_format_compose_build_empty() {
        let out = format_compose_build("");
        assert!(!out.is_empty(), "即使输入为空也应产生输出");
    }

    // ── compact_ports（现有函数，之前未覆盖测试） ──────

    #[test]
    fn test_compact_ports_empty() {
        assert_eq!(compact_ports(""), "-");
    }

    #[test]
    fn test_compact_ports_single() {
        let result = compact_ports("0.0.0.0:8080->80/tcp");
        assert!(result.contains("8080"));
    }

    #[test]
    fn test_compact_ports_many() {
        let result = compact_ports(
            "0.0.0.0:80->80/tcp, 0.0.0.0:443->443/tcp, 0.0.0.0:8080->8080/tcp, 0.0.0.0:9090->9090/tcp",
        );
        assert!(result.contains("..."), "端口超过 3 个时应截断");
    }
}
