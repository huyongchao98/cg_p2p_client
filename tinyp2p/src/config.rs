use std::{fmt, str::FromStr};

use libp2p::{
    identity::{ed25519, Keypair},
    multiaddr, Multiaddr, PeerId,
};
use libp2p::request_response::ProtocolSupport;
use serde::{Deserialize, Serialize};

use crate::error::P2pError;

/// P2p Configuration.
#[derive(Clone, Debug, Default)]
pub struct P2pConfig {
    /// The address to listen on.
    pub addr: String,
    /// Secret to generate the local keypair.
    /// If not provided, a random secret will be generated.
    pub secret: Option<String>,
    /// Bootstrap node to discover the peers in the network.
    /// If not provided, the node will start as a boot node.
    pub boot_node: Option<PeerIdWithMultiaddr>,
    pub boot_node_required: bool,
    /// The interval in seconds to discover the peers in the network.
    pub discovery_interval: Option<u64>,
    /// The topics to subscribe to.
    pub pubsub_topics: Vec<String>,
    /// Configuration for the request-response protocol.
    pub req_resp: Option<ReqRespConfig>,
}

/// Configuration for the request-response protocol.
#[derive(Clone, Debug, Default)]
pub struct ReqRespConfig {
    pub support: Option<ProtocolSupport>,
    /// Connection keep-alive time in seconds.
    pub connection_keep_alive: Option<u64>,
    /// Request timeout in seconds.
    pub request_timeout: Option<u64>,
    /// Maximum size of an inbound request.
    pub max_request_size: Option<usize>,
    /// Maximum size of an inbound response.
    pub max_response_size: Option<usize>,
}

impl P2pConfig {
    /// Generate a keypair from the secret.
    pub fn gen_keypair(&self) -> Result<Keypair, P2pError> {
        let secret = match &self.secret {
            Some(secret) => {
                let decoded = bs58::decode(secret)
                    .into_vec()
                    .map_err(|err| P2pError::InvalidSecretKey(err.to_string()))?;

                ed25519::SecretKey::try_from_bytes(decoded)
                    .map_err(|err| P2pError::InvalidSecretKey(err.to_string()))?
            }
            None => ed25519::SecretKey::generate(),
        };

        Ok(ed25519::Keypair::from(secret).into())
    }
}

/// Peer ID with multiaddress.
///
/// This struct represents a decoded version of a multiaddress that ends with `/p2p/<peerid>`.
///
/// # Example
///
/// ```
/// use tinyp2p_old::config::PeerIdWithMultiaddr;
/// let addr: PeerIdWithMultiaddr =
///     "/ip4/127.0.0.1/tcp/34567/p2p/12D3KooWSoC2ngFnfgSZcyJibKmZ2G58kbFcpmSPSSvDxeqkBLJc".parse().unwrap();
/// assert_eq!(addr.peer_id().to_base58(), "12D3KooWSoC2ngFnfgSZcyJibKmZ2G58kbFcpmSPSSvDxeqkBLJc");
/// assert_eq!(addr.address().to_string(), "/ip4/127.0.0.1/tcp/34567");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(try_from = "String", into = "String")]
pub struct PeerIdWithMultiaddr(pub PeerId, pub Multiaddr);

impl PeerIdWithMultiaddr {
    pub fn peer_id(&self) -> PeerId {
        self.0
    }

    pub fn address(&self) -> Multiaddr {
        self.1.clone()
    }
}

impl fmt::Display for PeerIdWithMultiaddr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let proto = multiaddr::Protocol::P2p(self.0);
        let p2p_addr = self.1.clone().with(proto);

        fmt::Display::fmt(&p2p_addr, f)
    }
}

impl FromStr for PeerIdWithMultiaddr {
    type Err = P2pError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (peer_id, multiaddr) = parse_str_addr(s)?;
        Ok(Self(peer_id, multiaddr))
    }
}

impl From<PeerIdWithMultiaddr> for String {
    fn from(ma: PeerIdWithMultiaddr) -> String {
        format!("{}", ma)
    }
}

impl TryFrom<String> for PeerIdWithMultiaddr {
    type Error = P2pError;

    fn try_from(string: String) -> Result<Self, Self::Error> {
        string.parse()
    }
}

fn parse_str_addr(addr_str: &str) -> Result<(PeerId, Multiaddr), P2pError> {
    let mut addr: Multiaddr = addr_str.parse()?;

    let peer_id = match addr.pop() {
        Some(multiaddr::Protocol::P2p(peer_id)) => peer_id,
        _ => return Err(P2pError::InvalidPeerId),
    };

    Ok((peer_id, addr))
}