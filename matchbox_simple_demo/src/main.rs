use futures::{select, FutureExt, StreamExt};
use futures_timer::Delay;
use log::info;
use matchbox_socket::{WebRtcSocket, WebRtcSocketEvent};
use std::time::{Duration, Instant};

const DEFAULT_BASE_URL: &str = "ws://localhost:3536";

#[cfg(target_arch = "wasm32")]
fn main() {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Debug).unwrap();

    wasm_bindgen_futures::spawn_local(async_main(DEFAULT_BASE_URL));
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "matchbox_simple_demo=info,matchbox_socket=info");
    }
    pretty_env_logger::init();

    let base_url = std::env::args().nth(1);
    let base_url = base_url.as_deref().unwrap_or(DEFAULT_BASE_URL);

    async_main(base_url).await
}

async fn async_main(base_url: &str) {
    info!("Connecting to matchbox");
    let (mut socket, loop_fut) = WebRtcSocket::new(&format!("{}/example_room", base_url));

    info!("my id is {:?}", socket.id());

    let loop_fut = loop_fut.fuse();
    futures::pin_mut!(loop_fut);

    let mut peers: Vec<String> = vec![];
    let mut tx = socket.sender().clone();
    let mut rx_stream = socket.async_receive().fuse();

    let timeout = Delay::new(Duration::from_millis(1000));
    futures::pin_mut!(timeout);

    let start_time = Instant::now();

    loop {
        select! {
            _ = (&mut timeout).fuse() => {
                for peer in &peers {
                    info!("Pinging {}", peer);

                    let mut ping = vec![0u8; 4 + 16].into_boxed_slice();
                    ping[..4].copy_from_slice(b"ping");
                    ping[4..].copy_from_slice(&start_time.elapsed().as_nanos().to_le_bytes());
                    tx.send(ping, peer);
                }

                timeout.reset(Duration::from_millis(1000));
            }

            ev = rx_stream.select_next_some() => {
                match ev {
                    WebRtcSocketEvent::NewPeer { id } => {
                        info!("New peer {}", id);
                        peers.push(id);
                    }

                    WebRtcSocketEvent::Message { from, contents } => {
                        if contents.starts_with(b"ping") {
                            info!("Received ping from {}, sending pong", from);
                            let mut reply = contents.clone();
                            // Reply with 'pong' and the same timestamp
                            reply[1] = b'o';
                            tx.send(reply, from);
                        } else if contents.starts_with(b"pong") {
                            let ping_ts = u128::from_le_bytes(contents[4..4 + 16].try_into().unwrap());
                            let ping_ts = start_time + Duration::from_nanos(ping_ts.try_into().unwrap());
                            let rtt = ping_ts.elapsed();
                            info!("Received pong from {}, rtt = {:?}", from, rtt);
                        } else {
                            info!("{}: {:?}", from, contents);
                        }
                    }
                }
            }

            _ = &mut loop_fut => {
                break;
            }
        }
    }

    info!("Done");
}
