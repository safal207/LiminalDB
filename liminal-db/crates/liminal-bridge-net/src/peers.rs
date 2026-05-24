use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use ed25519_dalek::Signer;
use ed25519_dalek::{Keypair, PublicKey, Signature, Verifier};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::broadcast;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PeerId(#[serde(with = "crate::peers::serde_peer_id")] [u8; 32]);

impl PeerId {
    pub fn new(bytes: [u8; 32]) -> Self {
        PeerId(bytes)
    }

    pub fn from_keypair(key: &Keypair) -> Self {
        PeerId(key.public.to_bytes())
    }

    pub fn from_public_key(key: &PublicKey) -> Self {
        PeerId(key.to_bytes())
    }

    pub fn to_bytes(self) -> [u8; 32] {
        self.0
    }

    pub fn public_key(&self) -> Result<PublicKey, ed25519_dalek::SignatureError> {
        PublicKey::from_bytes(&self.0)
    }
}

impl fmt::Debug for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PeerId({})", hex::encode(self.0))
    }
}

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PeerInfo {
    pub id: PeerId,
    pub url: String,
    pub namespace: String,
    pub credence: f32,
    pub last_seen_ms: u64,
}

impl PeerInfo {
    pub fn with_last_seen(mut self, millis: u64) -> Self {
        self.last_seen_ms = millis;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GossipEventKind {
    Registered,
    Summary,
    Expired,
}

#[derive(Clone, Debug)]
pub struct GossipEvent {
    pub peer: PeerId,
    pub kind: GossipEventKind,
    pub info: Option<PeerInfo>,
}

pub struct GossipManager {
    local_peer: PeerId,
    peers: RwLock<HashMap<PeerId, PeerInfo>>,
    gossip_interval: Duration,
    heartbeat_timeout: Duration,
    tx: broadcast::Sender<GossipEvent>,
}

impl GossipManager {
    pub fn new(local_peer: PeerId) -> Self {
        let (tx, _rx) = broadcast::channel(64);
        GossipManager {
            local_peer,
            peers: RwLock::new(HashMap::new()),
            gossip_interval: Duration::from_secs(5),
            heartbeat_timeout: Duration::from_secs(45),
            tx,
        }
    }

    pub fn with_gossip_interval(mut self, interval: Duration) -> Self {
        self.gossip_interval = interval;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.heartbeat_timeout = timeout;
        self
    }

    pub fn register_peer(&self, info: PeerInfo) {
        let mut guard = self.peers.write();
        guard.insert(info.id, info.clone());
        let _ = self.tx.send(GossipEvent {
            peer: info.id,
            kind: GossipEventKind::Registered,
            info: Some(info),
        });
    }

    pub fn update_summary(&self, id: PeerId, last_seen_ms: u64, credence: Option<f32>) {
        let mut guard = self.peers.write();
        if let Some(existing) = guard.get_mut(&id) {
            existing.last_seen_ms = last_seen_ms;
            if let Some(value) = credence {
                existing.credence = value;
            }
            let _ = self.tx.send(GossipEvent {
                peer: id,
                kind: GossipEventKind::Summary,
                info: Some(existing.clone()),
            });
        }
    }

    pub fn remove_peer(&self, id: &PeerId) -> Option<PeerInfo> {
        let mut guard = self.peers.write();
        let removed = guard.remove(id);
        if let Some(info) = &removed {
            let _ = self.tx.send(GossipEvent {
                peer: info.id,
                kind: GossipEventKind::Expired,
                info: Some(info.clone()),
            });
        }
        removed
    }

    pub fn peers(&self) -> Vec<PeerInfo> {
        let mut peers: Vec<_> = self.peers.read().values().cloned().collect();
        peers.sort_by(|a, b| {
            b.credence
                .partial_cmp(&a.credence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        peers
    }

    pub fn local_peer(&self) -> PeerId {
        self.local_peer
    }

    pub fn subscribe(&self) -> broadcast::Receiver<GossipEvent> {
        self.tx.subscribe()
    }

    pub fn gossip_interval(&self) -> Duration {
        self.gossip_interval
    }

    pub fn expire_stale(&self, now_ms: u64) -> Vec<PeerInfo> {
        let mut guard = self.peers.write();
        let timeout_ms = self.heartbeat_timeout.as_millis() as u64;
        let peers_to_remove: Vec<_> = guard
            .iter()
            .filter(|(_, info)| now_ms.saturating_sub(info.last_seen_ms) > timeout_ms)
            .map(|(peer, _)| *peer)
            .collect();

        let mut removed_infos = Vec::new();
        for peer in peers_to_remove {
            if let Some(info) = guard.remove(&peer) {
                let _ = self.tx.send(GossipEvent {
                    peer: info.id,
                    kind: GossipEventKind::Expired,
                    info: Some(info.clone()),
                });
                removed_infos.push(info);
            }
        }
        removed_infos
    }
}

#[derive(Debug, Error)]
pub enum HandshakeError {
    #[error("namespace mismatch")]
    NamespaceMismatch,
    #[error("stale handshake")]
    Stale,
    #[error("invalid signature")]
    InvalidSignature,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Handshake {
    pub peer: PeerId,
    pub namespace: String,
    pub timestamp_ms: u64,
    pub nonce: [u8; 16],
    #[serde(with = "serde_bytes")]
    pub signature: Vec<u8>,
}

impl Handshake {
    pub fn sign(
        signer: &Keypair,
        namespace: impl Into<String>,
        timestamp_ms: u64,
        nonce: [u8; 16],
    ) -> Self {
        let peer = PeerId::from_keypair(signer);
        let namespace = namespace.into();
        let message = Self::message(&peer, &namespace, timestamp_ms, &nonce);
        let signature = signer.sign(&message);
        Handshake {
            peer,
            namespace,
            timestamp_ms,
            nonce,
            signature: signature.to_bytes().to_vec(),
        }
    }

    fn message(peer: &PeerId, namespace: &str, timestamp_ms: u64, nonce: &[u8; 16]) -> Vec<u8> {
        let mut payload = Vec::with_capacity(32 + 8 + nonce.len() + namespace.len());
        payload.extend_from_slice(&peer.to_bytes());
        payload.extend_from_slice(&timestamp_ms.to_be_bytes());
        payload.extend_from_slice(nonce);
        payload.extend_from_slice(namespace.as_bytes());
        payload
    }

    pub fn verify(
        &self,
        expected_namespace: &str,
        now_ms: u64,
        window_ms: u64,
    ) -> Result<(), HandshakeError> {
        if self.namespace != expected_namespace {
            return Err(HandshakeError::NamespaceMismatch);
        }
        if now_ms.saturating_sub(self.timestamp_ms) > window_ms {
            return Err(HandshakeError::Stale);
        }
        let verifying_key = self
            .peer
            .public_key()
            .map_err(|_| HandshakeError::InvalidSignature)?;
        let message = Self::message(&self.peer, &self.namespace, self.timestamp_ms, &self.nonce);
        let signature =
            Signature::from_bytes(&self.signature).map_err(|_| HandshakeError::InvalidSignature)?;
        verifying_key
            .verify(&message, &signature)
            .map_err(|_| HandshakeError::InvalidSignature)
    }
}

mod serde_peer_id {
    use super::*;
    use serde::de::Error as _;

    pub fn serialize<S>(value: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&hex::encode(value))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        let bytes = hex::decode(&raw).map_err(D::Error::custom)?;
        let slice: [u8; 32] = bytes
            .try_into()
            .map_err(|_| D::Error::custom("peer id must contain 32 bytes"))?;
        Ok(slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    #[test]
    fn handshake_roundtrip() {
        let signer = Keypair::generate(&mut OsRng);
        let namespace = "liminal";
        let nonce: [u8; 16] = rand::random();
        let now = 1_000_000u64;
        let handshake = Handshake::sign(&signer, namespace, now, nonce);
        handshake
            .verify(namespace, now + 1_000, 10_000)
            .expect("handshake should verify");
    }

    #[test]
    fn handshake_rejects_invalid_signature() {
        let signer = Keypair::generate(&mut OsRng);
        let nonce: [u8; 16] = rand::random();
        let mut handshake = Handshake::sign(&signer, "liminal", 5_000, nonce);
        handshake.signature[0] ^= 0xFF;
        let err = handshake
            .verify("liminal", 6_000, 10_000)
            .expect_err("should reject tampered signature");
        matches!(err, HandshakeError::InvalidSignature);
    }

    #[test]
    fn handshake_rejects_stale_messages() {
        let signer = Keypair::generate(&mut OsRng);
        let nonce: [u8; 16] = rand::random();
        let handshake = Handshake::sign(&signer, "liminal", 1_000, nonce);
        let err = handshake
            .verify("liminal", 50_000, 10_000)
            .expect_err("handshake should be stale");
        matches!(err, HandshakeError::Stale);
    }
}
