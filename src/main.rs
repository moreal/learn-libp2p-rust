use std::time::Duration;
use async_std::io;
use futures::prelude::*;
use futures::select;
use libp2p::gossipsub::GossipsubEvent;
use libp2p::gossipsub::IdentTopic as Topic;
use libp2p::gossipsub::{MessageAuthenticity, ValidationMode, GossipsubMessage, MessageId};
use libp2p::{identity, Multiaddr, PeerId, gossipsub};
use libp2p::swarm::{Swarm, SwarmEvent};
use std::error::Error;

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("Local peer id: {:?}", local_peer_id);

    let transport = libp2p::development_transport(local_key.clone()).await?;

    let topic = Topic::new("test-net");
    let mut swarm = {
        let gossipsub_config = gossipsub::GossipsubConfigBuilder::default()
                .heartbeat_interval(Duration::from_secs(10)) // This is set to aid debugging by not cluttering the log space
                .validation_mode(ValidationMode::Strict) // This sets the kind of message validation. The default is Strict (enforce message signing)
                .message_id_fn(|_: &GossipsubMessage| {
                    MessageId::from(uuid::Uuid::new_v4().to_hyphenated().to_string())
                }) // content-address messages. No two messages of the
                // same content will be propagated.
                .build()
                .expect("Valid config");
        let mut gossipsub: gossipsub::Gossipsub =
            gossipsub::Gossipsub::new(MessageAuthenticity::Signed(local_key), gossipsub_config)
                .expect("Correct configuration");

        if let Some(explicit) = std::env::args().nth(2) {
            let explicit = explicit.clone();
            match explicit.parse() {
                Ok(id) => gossipsub.add_explicit_peer(&id),
                Err(err) => println!("Failed to parse explicit peer id: {:?}", err),
            }
        }

        gossipsub.subscribe(&topic).unwrap();

        Swarm::new(transport, gossipsub, local_peer_id)
    };

    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    if let Some(addr) = std::env::args().nth(1) {
        let remote: Multiaddr = addr.parse()?;
        swarm.dial(remote)?;
        println!("Dialed {}", addr)
    }

    let mut stdin = io::BufReader::new(io::stdin()).lines().fuse();

    loop {
        // https://docs.rs/futures/latest/futures/macro.select.html
        // Poll events
        select! {
            line = stdin.select_next_some() => {
                if let Err(e) = swarm
                    .behaviour_mut()
                    .publish(topic.clone(), line.expect("Stdin not to close").as_bytes())
                {
                    println!("Publish error: {:?}", e);
                }
            },
            event = swarm.select_next_some() => match event {
                SwarmEvent::Behaviour(GossipsubEvent::Message {
                    propagation_source: peer_id,
                    message_id: _,
                    message,
                }) => println!(
                    "[{}] {}",
                    peer_id,
                    String::from_utf8_lossy(&message.data),
                ),
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Listening on {:?}", address);
                }
                SwarmEvent::Behaviour(GossipsubEvent::Subscribed {
                    peer_id,
                    topic,
                }) if "test-net" == format!("{}", topic) => println!("New neighbor joined!: {}", peer_id),
                _ => {}
            }
        }
    }
}
