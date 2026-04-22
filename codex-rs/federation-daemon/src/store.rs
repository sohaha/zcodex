use std::fs;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use codex_federation_protocol::AckState;
use codex_federation_protocol::CARDS_DIRNAME;
use codex_federation_protocol::Envelope;
use codex_federation_protocol::EnvelopeAck;
use codex_federation_protocol::FederationCleanupReport;
use codex_federation_protocol::FederationStateLayout;
use codex_federation_protocol::FederationStateManifest;
use codex_federation_protocol::Heartbeat;
use codex_federation_protocol::INBOX_DIRNAME;
use codex_federation_protocol::INSTANCES_DIRNAME;
use codex_federation_protocol::InstanceCard;
use codex_federation_protocol::InstanceId;
use codex_federation_protocol::Lease;
use codex_federation_protocol::MAILBOXES_DIRNAME;

pub(crate) struct FederationStore {
    layout: FederationStateLayout,
}

impl FederationStore {
    pub(crate) fn new(state_root: impl Into<PathBuf>) -> Result<Self> {
        let layout = FederationStateLayout::new(state_root)?;
        Ok(Self { layout })
    }

    pub(crate) fn layout(&self) -> &FederationStateLayout {
        &self.layout
    }

    pub(crate) fn register_instance(&self, card: InstanceCard) -> Result<InstanceCard> {
        card.validate().map_err(anyhow::Error::msg)?;
        self.ensure_manifest(card.registered_at)?;
        self.write_instance_files(&card)?;
        Ok(card)
    }

    pub(crate) fn heartbeat_instance(
        &self,
        instance_id: InstanceId,
        lease: Lease,
        heartbeat: Heartbeat,
    ) -> Result<InstanceCard> {
        let mut card = self.read_card(instance_id)?;
        card.lease = lease;
        card.heartbeat = heartbeat;
        card.validate().map_err(anyhow::Error::msg)?;
        self.ensure_manifest(card.heartbeat.observed_at)?;
        self.write_instance_files(&card)?;
        Ok(card)
    }

    pub(crate) fn list_peers(
        &self,
        requester: Option<InstanceId>,
        now: i64,
    ) -> Result<Vec<InstanceCard>> {
        let mut peers = self.read_all_cards()?;
        peers.retain(|card| {
            card.lease.is_active_at(now)
                && requester
                    .map(|requester_id| requester_id != card.instance_id)
                    .unwrap_or(true)
        });
        peers.sort_by(|left, right| {
            left.display_name.cmp(&right.display_name).then_with(|| {
                left.instance_id
                    .to_string()
                    .cmp(&right.instance_id.to_string())
            })
        });
        Ok(peers)
    }

    pub(crate) fn send_envelope(&self, envelope: Envelope) -> Result<EnvelopeAck> {
        envelope.validate().map_err(anyhow::Error::msg)?;
        self.ensure_manifest(envelope.created_at)?;
        let envelope_path = self
            .layout
            .envelope_path(envelope.recipient, envelope.envelope_id);
        if envelope_path.exists() {
            let ack = EnvelopeAck {
                envelope_id: envelope.envelope_id,
                recipient: envelope.recipient,
                state: AckState::Rejected,
                updated_at: envelope.created_at,
                detail: Some("envelope already exists".to_string()),
            };
            self.persist_ack(&ack)?;
            return Ok(ack);
        }

        let recipient = self.read_card_if_exists(envelope.recipient)?;
        let Some(recipient) = recipient else {
            let ack = EnvelopeAck {
                envelope_id: envelope.envelope_id,
                recipient: envelope.recipient,
                state: AckState::Rejected,
                updated_at: envelope.created_at,
                detail: Some("recipient is not registered".to_string()),
            };
            self.persist_ack(&ack)?;
            return Ok(ack);
        };
        if !recipient.lease.is_active_at(envelope.created_at) {
            let ack = EnvelopeAck {
                envelope_id: envelope.envelope_id,
                recipient: envelope.recipient,
                state: AckState::Rejected,
                updated_at: envelope.created_at,
                detail: Some("recipient lease has expired".to_string()),
            };
            self.persist_ack(&ack)?;
            return Ok(ack);
        }

        self.layout.write_json(&envelope_path, &envelope)?;
        self.write_ack(EnvelopeAck {
            envelope_id: envelope.envelope_id,
            recipient: envelope.recipient,
            state: AckState::Accepted,
            updated_at: envelope.created_at,
            detail: None,
        })
    }

    pub(crate) fn read_inbox(&self, recipient: InstanceId, now: i64) -> Result<Vec<Envelope>> {
        let mut envelopes = self.read_envelopes_for_recipient(recipient)?;
        envelopes.retain(|envelope| envelope.expires_at > now);
        envelopes.sort_by(|left, right| left.created_at.cmp(&right.created_at));
        Ok(envelopes)
    }

    pub(crate) fn write_ack(&self, ack: EnvelopeAck) -> Result<EnvelopeAck> {
        ack.validate().map_err(anyhow::Error::msg)?;
        self.ensure_manifest(ack.updated_at)?;
        self.persist_ack(&ack)?;
        if ack.state != AckState::Accepted {
            remove_file_if_exists(&self.layout.envelope_path(ack.recipient, ack.envelope_id))?;
        }
        Ok(ack)
    }

    fn persist_ack(&self, ack: &EnvelopeAck) -> Result<()> {
        self.layout
            .write_json(&self.layout.ack_path(ack.envelope_id), ack)?;
        Ok(())
    }

    pub(crate) fn cleanup(&self, now: i64) -> Result<FederationCleanupReport> {
        self.ensure_manifest(now)?;
        let mut report = FederationCleanupReport::default();
        for card in self.read_all_cards()? {
            if !card.lease.is_active_at(now) {
                self.remove_instance_files(card.instance_id)?;
                merge_report(
                    &mut report,
                    self.expire_mailbox(card.instance_id, now, "recipient lease expired")?,
                );
                report.expired_instances_removed += 1;
            }
        }

        for mailbox_dir in read_directory_paths(&self.layout.root().join(MAILBOXES_DIRNAME))? {
            let inbox_dir = mailbox_dir.join(INBOX_DIRNAME);
            for envelope_path in read_json_paths(&inbox_dir)? {
                let envelope: Envelope = self.layout.read_json(&envelope_path)?;
                if envelope.expires_at <= now {
                    remove_file_if_exists(&envelope_path)?;
                    report.expired_envelopes_removed += 1;
                    report.acks_updated +=
                        self.write_expired_ack(&envelope, now, "envelope expired during cleanup")?;
                }
            }
        }

        Ok(report)
    }

    fn ensure_manifest(&self, created_at: i64) -> Result<()> {
        let manifest_path = self.layout.manifest_path();
        if manifest_path.exists() {
            let manifest: FederationStateManifest = self.layout.read_json(&manifest_path)?;
            manifest.validate().map_err(anyhow::Error::msg)?;
            return Ok(());
        }
        self.layout
            .write_json(&manifest_path, &FederationStateManifest::new(created_at))?;
        Ok(())
    }

    fn write_instance_files(&self, card: &InstanceCard) -> Result<()> {
        self.layout
            .write_json(&self.layout.instance_card_path(card.instance_id), card)?;
        self.layout
            .write_json(&self.layout.lease_path(card.instance_id), &card.lease)?;
        self.layout.write_json(
            &self.layout.heartbeat_path(card.instance_id),
            &card.heartbeat,
        )?;
        Ok(())
    }

    fn read_card(&self, instance_id: InstanceId) -> Result<InstanceCard> {
        self.read_card_if_exists(instance_id)?
            .with_context(|| format!("instance is not registered: {instance_id}"))
    }

    fn read_card_if_exists(&self, instance_id: InstanceId) -> Result<Option<InstanceCard>> {
        let path = self.layout.instance_card_path(instance_id);
        if !path.exists() {
            return Ok(None);
        }
        let card: InstanceCard = self.layout.read_json(&path)?;
        card.validate().map_err(anyhow::Error::msg)?;
        Ok(Some(card))
    }

    fn read_all_cards(&self) -> Result<Vec<InstanceCard>> {
        let mut cards = Vec::new();
        for path in read_json_paths(
            &self
                .layout
                .root()
                .join(INSTANCES_DIRNAME)
                .join(CARDS_DIRNAME),
        )? {
            let card: InstanceCard = self.layout.read_json(&path)?;
            card.validate().map_err(anyhow::Error::msg)?;
            cards.push(card);
        }
        Ok(cards)
    }

    fn read_envelopes_for_recipient(&self, recipient: InstanceId) -> Result<Vec<Envelope>> {
        let mut envelopes = Vec::new();
        for path in read_json_paths(&self.layout.inbox_dir(recipient))? {
            let envelope: Envelope = self.layout.read_json(&path)?;
            envelope.validate().map_err(anyhow::Error::msg)?;
            envelopes.push(envelope);
        }
        Ok(envelopes)
    }

    fn expire_mailbox(
        &self,
        recipient: InstanceId,
        now: i64,
        detail: &str,
    ) -> Result<FederationCleanupReport> {
        let mut report = FederationCleanupReport::default();
        for envelope in self.read_envelopes_for_recipient(recipient)? {
            remove_file_if_exists(
                &self
                    .layout
                    .envelope_path(envelope.recipient, envelope.envelope_id),
            )?;
            report.expired_envelopes_removed += 1;
            report.acks_updated += self.write_expired_ack(&envelope, now, detail)?;
        }
        remove_dir_if_exists(&self.layout.mailbox_dir(recipient))?;
        Ok(report)
    }

    fn write_expired_ack(&self, envelope: &Envelope, now: i64, detail: &str) -> Result<u64> {
        let ack = EnvelopeAck {
            envelope_id: envelope.envelope_id,
            recipient: envelope.recipient,
            state: AckState::Expired,
            updated_at: now,
            detail: Some(detail.to_string()),
        };
        self.layout
            .write_json(&self.layout.ack_path(envelope.envelope_id), &ack)?;
        Ok(1)
    }

    fn remove_instance_files(&self, instance_id: InstanceId) -> Result<()> {
        remove_file_if_exists(&self.layout.instance_card_path(instance_id))?;
        remove_file_if_exists(&self.layout.lease_path(instance_id))?;
        remove_file_if_exists(&self.layout.heartbeat_path(instance_id))?;
        Ok(())
    }
}

fn merge_report(target: &mut FederationCleanupReport, incoming: FederationCleanupReport) {
    target.expired_instances_removed += incoming.expired_instances_removed;
    target.expired_envelopes_removed += incoming.expired_envelopes_removed;
    target.acks_updated += incoming.acks_updated;
}

fn read_directory_paths(path: &Path) -> Result<Vec<PathBuf>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(path).with_context(|| format!("read directory {}", path.display()))? {
        let entry = entry?;
        let entry_path = entry.path();
        if entry_path.is_dir() {
            paths.push(entry_path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn read_json_paths(path: &Path) -> Result<Vec<PathBuf>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(path).with_context(|| format!("read directory {}", path.display()))? {
        let entry = entry?;
        let entry_path = entry.path();
        if entry_path
            .extension()
            .and_then(|extension| extension.to_str())
            == Some("json")
        {
            paths.push(entry_path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn remove_dir_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path).with_context(|| format!("remove directory {}", path.display()))?;
    }
    Ok(())
}

fn remove_file_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path).with_context(|| format!("remove file {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use codex_federation_protocol::EnvelopePayload;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use super::FederationStore;
    use codex_federation_protocol::AckState;
    use codex_federation_protocol::Envelope;
    use codex_federation_protocol::EnvelopeId;
    use codex_federation_protocol::Heartbeat;
    use codex_federation_protocol::InstanceCard;
    use codex_federation_protocol::InstanceId;
    use codex_federation_protocol::Lease;

    #[test]
    fn register_and_list_peers_skip_requester_and_expired_instances() {
        let tempdir = TempDir::new().expect("tempdir");
        let store = FederationStore::new(tempdir.path()).expect("store");
        let active = test_card("alpha", "/workspace/a", 100, 60, 120);
        let expired = test_card("beta", "/workspace/b", 100, 10, 105);
        store
            .register_instance(active.clone())
            .expect("register active");
        store.register_instance(expired).expect("register expired");

        let peers = store
            .list_peers(Some(active.instance_id), 120)
            .expect("list peers");

        assert_eq!(peers, Vec::<InstanceCard>::new());
    }

    #[test]
    fn cleanup_expires_instances_and_messages() {
        let tempdir = TempDir::new().expect("tempdir");
        let store = FederationStore::new(tempdir.path()).expect("store");
        let recipient = test_card("recipient", "/workspace/r", 100, 10, 105);
        let sender = test_card("sender", "/workspace/s", 100, 60, 110);
        store
            .register_instance(recipient.clone())
            .expect("register recipient");
        store.register_instance(sender).expect("register sender");
        let envelope = Envelope {
            envelope_id: EnvelopeId::default(),
            sender: InstanceId::default(),
            recipient: recipient.instance_id,
            created_at: 104,
            expires_at: 200,
            payload: EnvelopePayload::TextTask {
                text: "run task".to_string(),
            },
        };
        let ack = store.send_envelope(envelope.clone()).expect("send");
        assert_eq!(ack.state, AckState::Accepted);

        let report = store.cleanup(120).expect("cleanup");
        let stored_ack: codex_federation_protocol::EnvelopeAck = store
            .layout()
            .read_json(&store.layout().ack_path(envelope.envelope_id))
            .expect("ack");

        assert_eq!(report.expired_instances_removed, 1);
        assert_eq!(report.expired_envelopes_removed, 1);
        assert_eq!(report.acks_updated, 1);
        assert_eq!(stored_ack.state, AckState::Expired);
        assert_eq!(
            store
                .read_inbox(recipient.instance_id, 120)
                .expect("read inbox after cleanup"),
            Vec::<Envelope>::new()
        );
    }

    fn test_card(
        name: &str,
        cwd: &str,
        issued_at: i64,
        ttl_secs: u32,
        heartbeat_at: i64,
    ) -> InstanceCard {
        let lease = Lease::new(issued_at, ttl_secs).expect("lease");
        InstanceCard {
            instance_id: InstanceId::default(),
            display_name: name.to_string(),
            role: Some("worker".to_string()),
            task_scope: Some("scope".to_string()),
            cwd: PathBuf::from(cwd),
            registered_at: issued_at,
            lease,
            heartbeat: Heartbeat::new(1, heartbeat_at),
        }
    }
}
