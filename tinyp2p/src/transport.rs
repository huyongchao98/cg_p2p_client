use libp2p::{
    core::{muxing::StreamMuxerBox, transport::Boxed, upgrade::Version},
    identity, noise, tcp, yamux, PeerId, Transport
};

/// Create a tokio-based TCP transport use noise for authenticated
/// encryption and Yamux for multiplexing of substreams on a TCP stream.
pub fn build_transport(keypair: identity::Keypair) -> Boxed<(PeerId, StreamMuxerBox)> {
    let noise_config = noise::Config::new(&keypair).expect("failed to construct the noise config");

    // potential support other transports for better nat traversal
    tcp::tokio::Transport::default()
        .upgrade(Version::V1Lazy)
        .authenticate(noise_config)
        .multiplex(yamux::Config::default())
        .boxed()
}