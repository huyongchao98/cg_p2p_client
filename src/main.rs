/*
 * @Author: huych 305856242@qq.com
 * @Date: 2023-09-03 18:34:58
 * @LastEditors: huych
 * @LastEditTime: 2023-09-09 18:03:45
 * @FilePath: \cg_p2p_client (copy)\src\main.rs
 * @Description: 这是默认设置,请设置`customMade`, 打开koroFileHeader查看配置 进行设置: https://github.com/OBKoro1/koro1FileHeader/wiki/%E9%85%8D%E7%BD%AE
 */
use serde::{Deserialize, Serialize};
use std::cell::{OnceCell, RefCell};
use std::rc::{Rc, Weak};
use std::str::FromStr;
use std::sync::RwLock;
use std::{collections::HashMap, fmt::Debug, hash::Hash, net::Ipv4Addr, time::Duration};

use log::info;
use pretty_env_logger;
use tinyp2p::{
    config::P2pConfig, error::P2pSetBootNodeSuccessTypes, Client, DialError, EventHandler,
    Multiaddr, PeerIdWithMultiaddr, Protocol, ProtocolSupport, ReqRespConfig, Server,
};

use tinyp2p::protocol::ResponseType;

use tokio::{
    select, task,
    task::{AbortHandle, JoinSet},
};

use libp2p::request_response::{self, OutboundFailure};

unsafe fn any_as_vec_u8<T: Sized>(p: &T) -> Vec<u8> {
    core::slice::from_raw_parts((p as *const T) as *const u8, core::mem::size_of::<T>()).to_vec()
}

unsafe fn vec_u8_to_any<'a, T: Sized + 'a>(v: Vec<u8>) -> &'a T {
    let (head, body, _tail) = v.align_to::<T>();
    assert!(head.is_empty(), "Data was not aligned");
    &body[0]
}

type RcCell<T> = Rc<RefCell<T>>;
type WeakCell<T> = Weak<RefCell<T>>;
type RwLockCell<T> = RwLock<RefCell<T>>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(try_from = "String", into = "String")]
pub struct WardBroadcastMessage {
    id: BroadcastId,
    message: String,
}

impl ToString for WardBroadcastMessage {
    fn to_string(&self) -> String {
        let mut out = self.id.to_string();
        out.push_str("\n");
        out.push_str(&self.message);
        out
    }
}

impl FromStr for WardBroadcastMessage {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut lines = s.to_string().lines();
        let id = lines.next().unwrap().parse::<BroadcastId>().unwrap();
        let message = lines.next().unwrap().to_string();
        Ok(WardBroadcastMessage { id, message })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(try_from = "String", into = "String")]
pub struct WardBroadcastSender {
    node: PeerIdWithMultiaddr,
    ward_id: String,
}

impl ToString for WardBroadcastSender {
    fn to_string(&self) -> String {
        let mut out = self.node.to_string();
        out.push_str("\n");
        out.push_str(&self.ward_id);
        out
    }
}

impl FromStr for WardBroadcastSender {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut lines = s.to_string().lines();
        let node = lines
            .next()
            .unwrap()
            .parse::<PeerIdWithMultiaddr>()
            .unwrap();
        let ward_id = lines.next().unwrap().to_string();
        Ok(WardBroadcastSender { node, ward_id })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(try_from = "String", into = "String")]
pub struct WardBroadcastRequest {
    sender: WardBroadcastSender,
    message: WardBroadcastMessage,
}

impl ToString for WardBroadcastRequest {
    fn to_string(&self) -> String {
        let mut out = self.sender.to_string();
        out.push_str("\n\n");
        out.push_str(&self.message.to_string());
        out
    }
}

impl FromStr for WardBroadcastRequest {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut fields = s.to_string().split("\n\n");
        let sender = fields
            .next()
            .unwrap()
            .parse::<WardBroadcastSender>()
            .unwrap();
        let message = fields
            .next()
            .unwrap()
            .parse::<WardBroadcastMessage>()
            .unwrap();
        Ok(WardBroadcastRequest { sender, message })
    }
}

pub enum WardInboundRequest {
    BroadcastComplete { id: BroadcastId },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BroadcastId(u64);

pub trait GuardianDataHandler: Debug + Send + 'static {
    // will generalize message
    fn process_new_notification(ward_id: String, message: String);
}

pub struct Guardian<D> {
    client: Client,
    server: Server<GuardianHandler<D>>,
    messages: HashMap<String, Vec<WardBroadcastMessage>>, // keys are serialized WardBroadcastSender's
    data_handler: OnceCell<D>,
}

impl<D: GuardianDataHandler> Guardian<D> {
    pub fn new(ip: Ipv4Addr, port: Protocol, boot_node_required: bool) -> RcCell<Self> {
        let mut addr = Multiaddr::from(ip);
        addr.push(port);
        let mut req_resp = ReqRespConfig {
            ..Default::default()
        };
        req_resp.support = Option::from(ProtocolSupport::Inbound);

        let config = P2pConfig {
            addr: addr.to_string(),
            pubsub_topics: vec!["notification".to_string()],
            req_resp: Option::from(req_resp),
            boot_node_required,
            ..Default::default()
        };
        let (client, mut server) = tinyp2p::new(config).unwrap();
        let out = Rc::new(RefCell::new(Self {
            client,
            server,
            messages: HashMap::new(),
            data_handler: OnceCell::new(),
        }));
        server.set_event_handler(GuardianHandler {
            parent: Rc::downgrade(&out),
        });

        out
    }

    pub fn set_data_handler(&mut self, handler: D) {
        self.data_handler.set(handler).unwrap();
    }

    pub fn set_boot_node(
        &self,
        boot_node: PeerIdWithMultiaddr,
    ) -> Result<P2pSetBootNodeSuccessTypes, DialError> {
        self.client.set_boot_node(boot_node)
    }

    pub fn get_current_addr(&mut self) -> PeerIdWithMultiaddr {
        let status = self.client.get_node_status();
        PeerIdWithMultiaddr(status.local_peer_id, status.listened_addresses[0]) // to be changed
    }

    pub fn broadcast_received(&mut self, topic: String, message: WardBroadcastRequest) {
        match topic {
            String::from("notification") => {
                let key = message.sender.to_string();
                if !self.messages.contains_key(&key) {
                    self.messages.get(&key)?.push(message.message);
                } else {
                    self.messages.insert(key, vec![message.message]);
                }
                if let Some(handler) = self.data_handler.get() {
                    handler
                }
                // link to data_handler
            }
            _ => {}
        }
    }

    pub fn complete_broadcast(&self, ward_id: String, id: BroadcastId) {
        let mut addr: Option<PeerIdWithMultiaddr> = None;
        for key in self.messages.keys() {
            let sender = key.parse::<WardBroadcastSender>().unwrap();
            if sender.ward_id == ward_id {
                addr = Option::from(sender.node);
                break;
            }
        }

        if let Some(addr) = addr {
            let request = WardInboundRequest::BroadcastComplete { id };
            info!(
                "📣 >>>> Outbound BroadcastComplete: closing broadcast {:?} from {:?}",
                request.id, addr
            );
            let response = self
                .client
                .blocking_request(&addr, unsafe { any_as_vec_u8(&request) })
                .unwrap();
            info!(
                "📣 <<<< Inbound response to BroadcastComplete: {:?}",
                String::from_utf8_lossy(&response)
            );
        }
    }

    pub fn run(&mut self) {
        // Run the p2p server
        self.server.run();
    }
}

#[derive(Debug)]
struct GuardianHandler<D: GuardianDataHandler> {
    parent: RwLockCell<Box<Guardian<D>>>,
}

impl<D: GuardianDataHandler> EventHandler for GuardianHandler<D> {
    fn handle_inbound_broadcast(&self, topic: String, message: Vec<u8>) {
        let request = String::from_utf8(message)
            .unwrap()
            .parse::<WardBroadcastRequest>()
            .unwrap();
        info!(
            "📣 <<<< Inbound broadcast: {:?} {:?}",
            topic, request.message
        );
        self.parent
            .upgrade()?
            .borrow_mut()
            .broadcast_received(topic, request);
    }

    fn handle_new_listen_addr(&self, peer_id: &tinyp2p::PeerId, addr: &Multiaddr) {}

    fn handle_listener_closed(&self, addrs: Vec<Multiaddr>) {}

    fn handle_identify(&self, peer_id: &tinyp2p::PeerId, addrs: Vec<Multiaddr>) {}

    fn handle_remove_peer(&self, peer_id: &tinyp2p::PeerId) {}

    fn handle_inbound_request(&self, request: Vec<u8>) -> Result<Vec<u8>, tinyp2p::P2pError> {
        Ok(request)
    }

    fn handle_outbound_failure(&self, error: OutboundFailure) {}

    fn handle_inbound_response(&self, response: ResponseType) {}
}

pub struct Ward {
    id: String,
    client: Client,
    server: Server<WardHandler>,
    counter: BroadcastId,
    set: JoinSet<()>,
    abort_handles: HashMap<BroadcastId, AbortHandle>,
}

impl Ward {
    pub fn new(
        id: String,
        ip: Ipv4Addr,
        port: Protocol,
        boot_node: PeerIdWithMultiaddr,
    ) -> RcCell<Self> {
        let mut addr = Multiaddr::from(ip);
        addr.push(port);
        let mut req_resp = ReqRespConfig {
            ..Default::default()
        };
        req_resp.support = Option::from(ProtocolSupport::Outbound);

        let mut config = P2pConfig {
            addr: addr.to_string(),
            pubsub_topics: vec![],
            req_resp: Option::from(req_resp),
            ..Default::default()
        };
        config.boot_node = Option::from(boot_node);
        let (client, mut server) = tinyp2p::new::<WardHandler>(config).unwrap();
        let out = Rc::new(RefCell::new(Self {
            id,
            client,
            server,
            counter: BroadcastId(0),
            set: JoinSet::new(),
            abort_handles: HashMap::new(),
        }));
        server.set_event_handler(WardHandler {
            parent: Rc::downgrade(&out),
        });
        out
    }

    pub fn get_current_addr(&mut self) -> PeerIdWithMultiaddr {
        let status = self.client.get_node_status();
        PeerIdWithMultiaddr(status.local_peer_id, status.listened_addresses[0]) // to be changed
    }

    async fn _broadcast(&mut self, id: BroadcastId, message: &str) {
        let dur = Duration::from_secs(13);
        loop {
            tokio::time::sleep(dur).await;
            info!(
                "📣 >>>> Outbound broadcast: {:?} {:?}",
                "notification",
                message.to_string()
            );
            let _ = self.client.broadcast(
                "notification".to_string(),
                WardBroadcastRequest {
                    sender: WardBroadcastSender {
                        node: self.get_current_addr(),
                        ward_id: self.id.clone(),
                    },
                    message: WardBroadcastMessage {
                        id,
                        message: message.to_string(),
                    },
                }
                .to_string()
                .as_bytes()
                .to_vec(),
            );
        }
    }

    pub fn broadcast(&mut self, message: &str) {
        self.abort_handles.insert(
            self.counter,
            self.set.spawn(self._broadcast(self.counter, message)),
        );
        self.counter += 1;
    }

    fn _end_broadcast(&mut self, id: &BroadcastId) {
        self.abort_handles.get(id)?.abort();
    }
}

#[derive(Debug)]
struct WardHandler {
    parent: RwLockCell<Box<Ward>>,
}

impl EventHandler for WardHandler {
    fn handle_inbound_request(&self, request: Vec<u8>) -> Result<Vec<u8>, tinyp2p::P2pError> {
        info!(
            "📣 <<<< Inbound request for ward: {:?}",
            String::from_utf8_lossy(request.as_slice())
        );

        let cmd = unsafe { vec_u8_to_any::<WardInboundRequest>(request) };
        return match cmd {
            WardInboundRequest::BroadcastComplete { id } => {
                self.parent.upgrade()?.borrow_mut()._end_broadcast(id);
                Ok(vec![])
            }
        };
    }

    fn handle_new_listen_addr(&self, peer_id: &libp2p::PeerId, addr: &Multiaddr) {}

    fn handle_listener_closed(&self, addrs: Vec<Multiaddr>) {}

    fn handle_identify(&self, peer_id: &libp2p::PeerId, addrs: Vec<Multiaddr>) {}

    fn handle_remove_peer(&self, peer_id: &libp2p::PeerId) {}

    fn handle_outbound_failure(&self, error: OutboundFailure) {}

    fn handle_inbound_response(&self, response: ResponseType) {}

    fn handle_inbound_broadcast(&self, topic: String, message: Vec<u8>) {}
}

/*
#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    //println!("{}", guardian.server.listened_addresses.len().to_string());

    // Periodically print the node status.
    //let client_clone = client.clone();
    //thread::spawn(move || get_node_status(client_clone));

    // Periodically send a request to one of the known peers.
    //let client_clone = client.clone();
    //thread::spawn(move || request(client_clone));

    // Periodically make a broadcast to the network.
    //
}


 */

#[derive(Debug)]
pub struct MockDataHandler();

impl GuardianDataHandler for MockDataHandler {
    fn process_new_notification(ward_id: String, message: String) {
        info!("processing id: {:?} message: {:?}", ward_id, message);
    }
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let mut guardian = Guardian::new(Ipv4Addr::new(0, 0, 0, 0), Protocol::Tcp(44444), false);
    guardian.borrow_mut().set_data_handler(MockDataHandler);
    tokio::spawn(guardian.borrow_mut().run());

    let mut ward = Ward::new(
        "123123124".to_string(),
        Ipv4Addr::new(0, 0, 0, 0),
        Protocol::Tcp(55555),
        guardian.borrow_mut().get_current_addr(),
    );
    ward.borrow_mut().broadcast("Hello there!");
}

/*
pub enum HandlerCommand {
    NotifyNewAddr {
        peer_id: PeerId,
        addrs: Vec<Multiaddr>,
    },
}

fn handle_command(cmd: HandlerCommand) {
    match cmd {
        HandlerCommand::NotifyNewAddr { peer_id, addrs } => {

        }
    }
}

pub struct Account {
    guardians: Vec<Guardian>,
    wards: Vec<Ward>,
    sender: UnboundedSender<HandlerCommand>,
    receiver: UnboundedReceiver<HandlerCommand>
}

impl Account {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        Self{guardians: vec![], wards: vec![], sender, receiver}
    }

    pub fn add_guardian(&mut self, ip: Ipv4Addr, port: Protocol) {
        let guardian =
        if self.guardians.len() == 0 {
            Guardian::new(ip, port.clone(), vec![], true, self.sender.clone())
        } else {
            // get topic list from wards
            Guardian::new(ip, port.clone(), vec![], true, self.sender.clone())
        };
        self.guardians.push(guardian);
    }

    pub fn on_guardian_leave(&mut self, addr: PeerIdWithMultiaddr) {
        let mut sub_guardians = vec![];
        let mut disjoint_guardians = vec![];
        for guardian in self.guardians {
            if let Some(node) = guardian.server.boot_node {
                if addr == node {
                    sub_guardians.push(&guardian);
                } else {
                    disjoint_guardians.push(&guardian);
                }
            }
        }
        let new_boot =
            if disjoint_guardians.len() == 0 {
                if sub_guardians.len() != 0 {
                    // set new_boot's required as false
                    // set all other sub_guardians' boot to new_boot
                    sub_guardians.get(0).unwrap().deref()
                } else {
                    null()
                }
            } else {
                // set all sub_guardians' boot to new_boot
                disjoint_guardians.get(0).unwrap().deref()
            };

        if !new_boot.is_null() {
            let status: NodeStatus = new_boot.client.get_status();
            let new_addr = PeerIdWithMultiaddr(status.)
        }

        for ward in self.wards {
            if let Some(node) = ward.server.boot_node {
                if addr == node {
                    if new_boot.is_null() {
                        // handle instead
                        panic!("No guardians remaining when wards still available!")
                    } else {
                        // try all the different nodes
                        ward.client.set_boot_node(new_boot.server.)
                    }
                }
            }
        }
    }

    pub fn add_ward() {
        // if guardians non-empty
            // happily add to list & register
        // if guardians empty
            // add to list & set state as pending
    }

    pub fn on_ward_leave() {
        // remove from list & destroy, simple
    }
}
 */
