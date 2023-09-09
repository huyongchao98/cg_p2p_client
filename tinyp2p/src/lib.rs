/*
 * @Author: huych
 * @Date: 2023-09-02 23:29:30
 * @LastEditors: huych
 * @LastEditTime: 2023-09-05 13:20:45
 * @FilePath: /cg_p2p_client (copy)/tinyp2p/src/lib.rs
 * @Description: 这是默认设置,请设置`customMade`, 打开koroFileHeader查看配置 进行设置: https://github.com/OBKoro1/koro1FileHeader/wiki/%E9%85%8D%E7%BD%AE
 */
pub mod config;
pub mod error;

pub mod protocol;
mod service;
mod transport;

pub use config::*;
pub use error::P2pError;
pub use service::{new, new_secret_key, Client, EventHandler, Server};

// Re-export libp2p types.
pub use libp2p::request_response::ProtocolSupport;
pub use libp2p::swarm::DialError;
pub use libp2p::{multiaddr::Protocol, Multiaddr, PeerId};
