use chrono::Utc;
use crossbeam_channel::{bounded, tick, unbounded, Receiver, Sender};
use defer_lite::defer;
use game_server::{Broadcast, TickableNpcSynchronization};
use parking_lot::{Mutex, MutexGuard, RwLock, RwLockReadGuard};
use protocol::{BufferSize, DisconnectReason, MAX_BUFFER_SIZE};
use serde::de::IgnoredAny;
use serde::Deserialize;
use std::cell::Cell;
use std::fs::File;
use std::io::Error;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use std::{env, panic, process};
use tokio::spawn;

use crate::channel_manager::{ChannelManager, ReceiveResult};
use crate::game_server::GameServer;
use crate::protocol::Channel;

mod channel_manager;
mod game_server;
mod http;
mod protocol;

thread_local! {
    pub static PROCESSED_CLIENT_ADDR: Cell<Option<SocketAddr>> = const { Cell::new(None) };
    pub static PROCESSED_CLIENT_GUID: Cell<Option<u32>> = const { Cell::new(None) };
}

pub fn log_info(message: &str) {
    let client = if let Some(addr) = PROCESSED_CLIENT_ADDR.get() {
        format!(" [client={addr}]")
    } else {
        "".to_string()
    };
    let guid = if let Some(guid) = PROCESSED_CLIENT_GUID.get() {
        format!(" [guid={guid}]")
    } else {
        "".to_string()
    };
    println!("{}{client}{guid}\t{message}", Utc::now().to_rfc3339());
}

static DEBUG_ENABLED: LazyLock<bool> = LazyLock::new(|| {
    env::var("RUST_LOG")
        .map(|value| value.to_lowercase() == "debug")
        .unwrap_or_default()
});

pub fn log_debug(message: &str) {
    if *DEBUG_ENABLED {
        log_info(message);
    }
}

#[macro_export]
macro_rules! info {
    () => {
        $crate::log_info("");
    };
    ($($arg:tt)*) => {{
        $crate::log_info(&format!($($arg)*))
    }};
}

#[macro_export]
macro_rules! debug {
    () => {
        $crate::log_debug("");
    };
    ($($arg:tt)*) => {{
        $crate::log_debug(&format!($($arg)*))
    }};
}

#[derive(Debug)]
pub enum ConfigError {
    Io(Error),
    Deserialize(serde_yaml::Error),
    ConstraintViolated(String),
}

impl From<Error> for ConfigError {
    fn from(value: Error) -> Self {
        ConfigError::Io(value)
    }
}

impl From<serde_yaml::Error> for ConfigError {
    fn from(value: serde_yaml::Error) -> Self {
        ConfigError::Deserialize(value)
    }
}

#[tokio::main]
async fn main() {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        default_hook(panic_info);
        process::exit(1);
    }));

    let config_dir = Path::new("config");
    let server_options =
        Arc::new(load_server_options(config_dir).expect("Unable to read server options"));
    server_options.validate();

    spawn(http::start(
        server_options.bind_ip,
        server_options.https_port,
        config_dir,
        Path::new("config/custom_assets"),
        PathBuf::from(".asset_cache"),
    ));
    let socket = UdpSocket::bind(SocketAddr::new(
        server_options.bind_ip,
        server_options.udp_port,
    ))
    .expect("couldn't bind to socket");
    info!("Hello, world!");

    let channel_manager = RwLock::new(ChannelManager::new(server_options.max_sessions));
    let game_server = GameServer::new(config_dir).unwrap();

    let channel_manager_arc = Arc::new(channel_manager);
    let socket_arc = Arc::new(socket);
    let game_server_arc = Arc::new(game_server);

    let (client_enqueue, client_dequeue) = bounded(server_options.max_sessions);
    let mut threads = spawn_receive_threads(
        server_options.receive_threads,
        &channel_manager_arc,
        &socket_arc,
        client_enqueue.clone(),
        MAX_BUFFER_SIZE,
        &server_options,
        &game_server_arc,
    );
    threads.append(&mut spawn_process_threads(
        server_options.process_threads,
        &channel_manager_arc,
        &socket_arc,
        client_enqueue.clone(),
        client_dequeue,
        server_options.clone(),
        &game_server_arc,
    ));

    let chunk_tick_dequeue = tick(Duration::from_millis(
        server_options.chunk_tick_period_millis,
    ));
    spawn_chunk_tick_threads(
        &channel_manager_arc,
        chunk_tick_dequeue,
        client_enqueue.clone(),
        &server_options,
        &game_server_arc,
    );

    let matchmaking_tick_dequeue = tick(Duration::from_millis(
        server_options.matchmaking_tick_period_millis,
    ));
    spawn_matchmaking_tick_thread(
        &channel_manager_arc,
        matchmaking_tick_dequeue,
        client_enqueue.clone(),
        &server_options,
        &game_server_arc,
    );

    let mainigame_tick_dequeue = tick(Duration::from_millis(
        server_options.minigame_tick_period_millis,
    ));
    spawn_minigame_tick_threads(
        &channel_manager_arc,
        mainigame_tick_dequeue,
        client_enqueue.clone(),
        &server_options,
        &game_server_arc,
    );

    let cleanup_tick_dequeue = tick(Duration::from_millis(
        server_options.channel_cleanup_period_millis,
    ));
    spawn_cleanup_thread(
        &channel_manager_arc,
        &socket_arc,
        cleanup_tick_dequeue,
        client_enqueue,
        &server_options,
        &game_server_arc,
    );

    for thread in threads {
        thread.join().expect("Thread exited with error");
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerOptions {
    #[serde(default)]
    pub comment: IgnoredAny,
    pub bind_ip: IpAddr,
    pub udp_port: u16,
    pub https_port: u16,
    pub crc_length: u8,
    pub allow_packet_compression: bool,
    pub receive_threads: u16,
    pub process_threads: u16,
    pub max_sessions: usize,
    pub process_packets_per_cycle: u8,
    pub send_packets_per_cycle: u8,
    pub packet_recency_limit: u16,
    pub max_received_packets_queued: usize,
    pub max_unacknowledged_packets_queued: usize,
    pub max_defragmented_packet_bytes: u32,
    pub max_decompressed_packet_bytes: usize,
    pub default_millis_until_resend: u64,
    pub max_round_trip_entries: usize,
    pub desired_resend_pct: u8,
    pub max_millis_until_resend: u64,
    pub chunk_tick_period_millis: u64,
    pub chunk_tick_threads: u16,
    pub matchmaking_tick_period_millis: u64,
    pub minigame_tick_period_millis: u64,
    pub minigame_tick_threads: u16,
    pub channel_cleanup_period_millis: u64,
    pub channel_inactive_timeout_millis: u64,
    pub min_client_version: Option<String>,
    pub max_client_version: Option<String>,
}

impl ServerOptions {
    fn validate(&self) {
        if self.crc_length > 4 || self.crc_length < 1 {
            panic!("crc_length must be between 1 and 4 (inclusive)");
        }

        if self.desired_resend_pct >= 100 {
            panic!("desired_resend_pct must be less than 100")
        }
    }

    fn allows_client_version(&self, client_version: &String) -> bool {
        if let Some(min) = &self.min_client_version {
            if client_version < min {
                return false;
            }
        }

        if let Some(max) = &self.max_client_version {
            if client_version > max {
                return false;
            }
        }

        true
    }
}

fn load_server_options(config_dir: &Path) -> Result<ServerOptions, ConfigError> {
    let mut file = File::open(config_dir.join("server.yaml"))?;
    Ok(serde_yaml::from_reader(&mut file)?)
}

fn spawn_receive_threads(
    threads: u16,
    channel_manager: &Arc<RwLock<ChannelManager>>,
    socket: &Arc<UdpSocket>,
    client_enqueue: Sender<SocketAddr>,
    initial_buffer_size: BufferSize,
    server_options: &Arc<ServerOptions>,
    game_server: &Arc<GameServer>,
) -> Vec<JoinHandle<()>> {
    (0..threads)
        .map(|_| {
            let channel_manager = channel_manager.clone();
            let socket = socket.clone();
            let client_enqueue = client_enqueue.clone();
            let server_options = server_options.clone();
            let game_server = game_server.clone();

            thread::spawn(move || loop {
                receive_once(
                    initial_buffer_size,
                    &channel_manager,
                    &socket,
                    &client_enqueue,
                    &server_options,
                    &game_server,
                );
            })
        })
        .collect()
}

fn receive_once(
    initial_buffer_size: BufferSize,
    channel_manager: &Arc<RwLock<ChannelManager>>,
    socket: &Arc<UdpSocket>,
    client_enqueue: &Sender<SocketAddr>,
    server_options: &Arc<ServerOptions>,
    game_server: &Arc<GameServer>,
) {
    let mut buf = [0; MAX_BUFFER_SIZE as usize];
    if let Ok((len, src)) = socket.recv_from(&mut buf) {
        let recv_data = &buf[0..len];

        loop {
            let read_handle = channel_manager.read();
            let receive_result =
                read_handle.receive(client_enqueue.clone(), &src, recv_data, server_options);
            if receive_result == ReceiveResult::CreateChannelFirst {
                info!("Creating channel for {}", src);
                drop(read_handle);

                let new_channel = Channel::new(
                    src,
                    initial_buffer_size,
                    server_options.packet_recency_limit,
                    Duration::from_millis(server_options.default_millis_until_resend),
                    server_options.max_round_trip_entries,
                    server_options.desired_resend_pct,
                    Duration::from_millis(server_options.max_millis_until_resend),
                );
                let mut write_handle = channel_manager.write();

                match write_handle.insert(&src, new_channel) {
                    Ok(possible_previous_channel) => {
                        if let Some(previous_channel) = possible_previous_channel {
                            info!("Client {} reconnected, dropping old channel", src);
                            if let Some(guid) = write_handle.guid(&src) {
                                let log_out_broadcasts = log_out_and_disconnect(
                                    Some(DisconnectReason::NewConnectionAttempt),
                                    guid,
                                    &[],
                                    previous_channel,
                                    game_server,
                                    socket,
                                    server_options,
                                );
                                write_handle.broadcast(
                                    client_enqueue.clone(),
                                    log_out_broadcasts,
                                    server_options,
                                );
                            }
                        }
                    }
                    Err(err) => {
                        info!(
                            "Could not create channel because maximum of {} channels was reached",
                            err.0
                        );
                        let channel: Mutex<Channel> = err.1.into();
                        disconnect(
                            Some(DisconnectReason::ConnectionRefused),
                            &[recv_data],
                            channel.lock(),
                            socket,
                            server_options,
                        );
                    }
                }
            } else {
                break;
            }
        }
    }
}

fn spawn_process_threads(
    threads: u16,
    channel_manager: &Arc<RwLock<ChannelManager>>,
    socket: &Arc<UdpSocket>,
    client_enqueue: Sender<SocketAddr>,
    client_dequeue: Receiver<SocketAddr>,
    server_options: Arc<ServerOptions>,
    game_server: &Arc<GameServer>,
) -> Vec<JoinHandle<()>> {
    (0..threads)
        .map(|_| {
            let channel_manager = channel_manager.clone();
            let socket = socket.clone();
            let server_options = server_options.clone();
            let game_server = game_server.clone();
            let client_enqueue = client_enqueue.clone();
            let client_dequeue = client_dequeue.clone();

            thread::spawn(move || loop {
                process_once(
                    &channel_manager,
                    &socket,
                    &server_options,
                    &game_server,
                    &client_dequeue,
                    &client_enqueue,
                );
            })
        })
        .collect()
}

fn process_once(
    channel_manager: &Arc<RwLock<ChannelManager>>,
    socket: &Arc<UdpSocket>,
    server_options: &Arc<ServerOptions>,
    game_server: &Arc<GameServer>,
    client_dequeue: &Receiver<SocketAddr>,
    client_enqueue: &Sender<SocketAddr>,
) {
    defer! { PROCESSED_CLIENT_ADDR.set(None) }
    defer! { PROCESSED_CLIENT_GUID.set(None) }

    // Don't lock the channel manager until we have packets to process
    // to avoid deadlock with channel creation
    let src = client_dequeue
        .recv()
        .expect("Tried to dequeue client after queue channel disconnected");
    PROCESSED_CLIENT_ADDR.set(Some(src));

    let mut channel_manager_read_handle = channel_manager.read();
    PROCESSED_CLIENT_GUID.set(channel_manager_read_handle.guid(&src));
    let mut channel_handle =
        if let Some(channel_handle) = lock_channel(&channel_manager_read_handle, &src) {
            channel_handle
        } else {
            return;
        };

    let packets_to_send = channel_manager_read_handle
        .send_next(&mut channel_handle, server_options.send_packets_per_cycle);
    send_packets(&packets_to_send, &src, socket);

    let packets_for_game_server = channel_manager_read_handle.process_next(
        &mut channel_handle,
        server_options.process_packets_per_cycle,
        server_options,
    );

    let mut broadcasts = Vec::new();
    for packet in packets_for_game_server {
        if let Some(guid) = channel_manager_read_handle.guid(&src) {
            match game_server.process_packet(guid, packet) {
                Ok(mut new_broadcasts) => broadcasts.append(&mut new_broadcasts),
                Err(err) => match err.log_level() {
                    game_server::LogLevel::Debug => {
                        debug!("Unable to process packet for client {}: {}", src, err)
                    }
                    game_server::LogLevel::Info => {
                        info!("Unable to process packet for client {}: {}", src, err)
                    }
                },
            }
        } else {
            match game_server.authenticate(packet) {
                Ok((guid, client_version)) => {
                    if !server_options.allows_client_version(&client_version) {
                        info!(
                            "Disconnecting client {} that attempted to authenticate with disallowed version {}",
                            channel_handle.addr, client_version
                        );
                        disconnect(
                            Some(DisconnectReason::Application),
                            &[],
                            channel_handle,
                            socket,
                            server_options,
                        );
                        return;
                    }

                    drop(channel_handle);
                    drop(channel_manager_read_handle);

                    let mut channel_manager_write_handle = channel_manager.write();
                    if let Some(existing_channel) =
                        channel_manager_write_handle.authenticate(&src, guid)
                    {
                        info!("Client {} logged in as an already logged-in player {}, disconnecting existing client", src, guid);
                        broadcasts.append(&mut log_out_and_disconnect(
                            Some(DisconnectReason::NewConnectionAttempt),
                            guid,
                            &[],
                            existing_channel,
                            game_server,
                            socket,
                            server_options,
                        ));
                    }

                    match game_server.log_in(guid) {
                        Ok(mut log_in_broadcasts) => broadcasts.append(&mut log_in_broadcasts),
                        Err(err) => {
                            info!(
                                "Unable to log in player {} on client {}: {}",
                                guid, src, err
                            );
                            if let Some(channel) = channel_manager_write_handle.get_by_addr(&src) {
                                disconnect_or_log_err(
                                    &mut channel.lock(),
                                    DisconnectReason::Application,
                                    socket,
                                );
                            }
                        }
                    };
                    drop(channel_manager_write_handle);

                    channel_manager_read_handle = channel_manager.read();
                    channel_handle = if let Some(channel_handle) =
                        lock_channel(&channel_manager_read_handle, &src)
                    {
                        channel_handle
                    } else {
                        return;
                    };
                }
                Err(err) => {
                    info!("Unable to process login packet for client {}: {}", src, err);
                    disconnect_or_log_err(
                        &mut channel_handle,
                        DisconnectReason::Application,
                        socket,
                    );
                }
            }
        }
    }

    // Re-enqueue this address for another thread to pick up if there is still more processing to be done
    if channel_handle.needs_processing() {
        client_enqueue
            .send(src)
            .expect("Tried to enqueue client after queue channel disconnected");
    }

    drop(channel_handle);
    channel_manager_read_handle.broadcast(client_enqueue.clone(), broadcasts, server_options);
}

fn spawn_chunk_tick_threads(
    channel_manager: &Arc<RwLock<ChannelManager>>,
    chunk_tick_dequeue: Receiver<Instant>,
    client_enqueue: Sender<SocketAddr>,
    server_options: &Arc<ServerOptions>,
    game_server: &Arc<GameServer>,
) {
    let (chunks_enqueue, chunks_dequeue) = unbounded();
    let (done_enqueue, done_dequeue) = unbounded();

    for _ in 0..server_options.chunk_tick_threads {
        let chunks_dequeue = chunks_dequeue.clone();
        let done_enqueue = done_enqueue.clone();
        let client_enqueue = client_enqueue.clone();
        let channel_manager = channel_manager.clone();
        let server_options = server_options.clone();
        let game_server = game_server.clone();

        thread::spawn(move || loop {
            let (instance_guid, chunk, synchronization) = chunks_dequeue
                .recv()
                .expect("Chunk tick channel disconnected");
            let broadcasts = game_server.tick_single_chunk(
                Instant::now(),
                instance_guid,
                chunk,
                synchronization,
            );
            channel_manager
                .read()
                .broadcast(client_enqueue.clone(), broadcasts, &server_options);

            done_enqueue
                .send(())
                .expect("Chunk tick done channel disconnected");
        });
    }

    let game_server = game_server.clone();

    // Always spawn the control thread
    thread::spawn(move || loop {
        chunk_tick_dequeue
            .recv()
            .expect("Chunk tick channel disconnected");

        let tasks = game_server.enqueue_tickable_chunks(
            TickableNpcSynchronization::Unsynchronized,
            chunks_enqueue.clone(),
        );
        let mut done_signals = 0;
        while done_signals < tasks {
            done_dequeue
                .recv()
                .expect("Chunk tick done channel disconnected");
            done_signals += 1;
        }

        let tasks = game_server.enqueue_tickable_chunks(
            TickableNpcSynchronization::Synchronized,
            chunks_enqueue.clone(),
        );
        let mut done_signals = 0;
        while done_signals < tasks {
            done_dequeue
                .recv()
                .expect("Chunk tick done channel disconnected");
            done_signals += 1;
        }
    });
}

fn spawn_matchmaking_tick_thread(
    channel_manager: &Arc<RwLock<ChannelManager>>,
    matchmaking_tick_dequeue: Receiver<Instant>,
    client_enqueue: Sender<SocketAddr>,
    server_options: &Arc<ServerOptions>,
    game_server: &Arc<GameServer>,
) {
    let client_enqueue = client_enqueue.clone();
    let channel_manager = channel_manager.clone();
    let server_options = server_options.clone();
    let game_server = game_server.clone();

    thread::spawn(move || loop {
        matchmaking_tick_dequeue
            .recv()
            .expect("Matchmaking tick channel disconnected");

        let broadcasts = game_server.tick_matchmaking_groups();
        channel_manager
            .read()
            .broadcast(client_enqueue.clone(), broadcasts, &server_options);
    });
}

fn spawn_minigame_tick_threads(
    channel_manager: &Arc<RwLock<ChannelManager>>,
    minigame_tick_dequeue: Receiver<Instant>,
    client_enqueue: Sender<SocketAddr>,
    server_options: &Arc<ServerOptions>,
    game_server: &Arc<GameServer>,
) {
    let (minigames_enqueue, minigames_dequeue) = unbounded();
    let (done_enqueue, done_dequeue) = unbounded();

    for _ in 0..server_options.minigame_tick_threads {
        let minigames_dequeue = minigames_dequeue.clone();
        let done_enqueue = done_enqueue.clone();
        let client_enqueue = client_enqueue.clone();
        let channel_manager = channel_manager.clone();
        let server_options = server_options.clone();
        let game_server = game_server.clone();

        thread::spawn(move || loop {
            let group = minigames_dequeue
                .recv()
                .expect("Minigame tick channel disconnected");
            let broadcasts = game_server.tick_minigame(Instant::now(), group);
            channel_manager
                .read()
                .broadcast(client_enqueue.clone(), broadcasts, &server_options);

            done_enqueue
                .send(())
                .expect("Minigame tick done channel disconnected");
        });
    }

    let game_server = game_server.clone();

    // Always spawn the control thread
    thread::spawn(move || loop {
        minigame_tick_dequeue
            .recv()
            .expect("Minigame tick channel disconnected");

        let tasks = game_server.enqueue_tickable_minigames(minigames_enqueue.clone());
        let mut done_signals = 0;
        while done_signals < tasks {
            done_dequeue
                .recv()
                .expect("Minigame tick done channel disconnected");
            done_signals += 1;
        }
    });
}

fn spawn_cleanup_thread(
    channel_manager: &Arc<RwLock<ChannelManager>>,
    socket: &Arc<UdpSocket>,
    cleanup_tick_dequeue: Receiver<Instant>,
    client_enqueue: Sender<SocketAddr>,
    server_options: &Arc<ServerOptions>,
    game_server: &Arc<GameServer>,
) {
    let channel_inactive_timeout =
        Duration::from_millis(server_options.channel_inactive_timeout_millis);
    let channel_manager = channel_manager.clone();
    let socket = socket.clone();
    let client_enqueue = client_enqueue.clone();
    let server_options = server_options.clone();
    let game_server = game_server.clone();
    thread::spawn(move || loop {
        cleanup_once(
            &cleanup_tick_dequeue,
            channel_inactive_timeout,
            &channel_manager,
            &socket,
            &client_enqueue,
            &server_options,
            &game_server,
        );
    });
}

fn cleanup_once(
    cleanup_tick_dequeue: &Receiver<Instant>,
    channel_inactive_timeout: Duration,
    channel_manager: &Arc<RwLock<ChannelManager>>,
    socket: &Arc<UdpSocket>,
    client_enqueue: &Sender<SocketAddr>,
    server_options: &Arc<ServerOptions>,
    game_server: &Arc<GameServer>,
) {
    cleanup_tick_dequeue
        .recv()
        .expect("Cleanup tick channel disconnected");
    let mut channel_manager_handle = channel_manager.write();
    let channels_to_disconnect = channel_manager_handle.drain_filter(|channel| {
        if channel.elapsed_since_last_receive() > channel_inactive_timeout {
            disconnect_or_log_err(channel, DisconnectReason::Timeout, socket);
        }
        !channel.connected()
    });

    let mut broadcasts = Vec::new();
    for (possible_guid, channel) in channels_to_disconnect {
        if let Some(guid) = possible_guid {
            broadcasts.append(&mut log_out_and_disconnect(
                None,
                guid,
                &[],
                channel,
                game_server,
                socket,
                server_options,
            ));
        }
    }

    channel_manager_handle.broadcast(client_enqueue.clone(), broadcasts, server_options);
}

fn lock_channel<'a>(
    channel_manager_read_handle: &'a RwLockReadGuard<ChannelManager>,
    addr: &'a SocketAddr,
) -> Option<MutexGuard<'a, Channel>> {
    channel_manager_read_handle
        .get_by_addr(addr)
        .map(|channel| channel.lock())
}

fn send_packet(buffer: &[u8], addr: &SocketAddr, socket: &Arc<UdpSocket>) {
    if let Err(err) = socket.send_to(buffer, addr) {
        info!("Unable to send packet to client {}: {}", addr, err);
    }
}

fn send_packets(packets: &[Vec<u8>], addr: &SocketAddr, socket: &Arc<UdpSocket>) {
    packets
        .iter()
        .for_each(|packet| send_packet(packet, addr, socket))
}

fn disconnect_or_log_err(
    channel_handle: &mut MutexGuard<Channel>,
    disconnect_reason: DisconnectReason,
    socket: &Arc<UdpSocket>,
) {
    match channel_handle.disconnect(disconnect_reason) {
        Ok(disconnect_packets) => send_packets(&disconnect_packets, &channel_handle.addr, socket),
        Err(err) => info!(
            "Unable to serialize disconnect packet for client {}: {:?}",
            channel_handle.addr, err
        ),
    }
}

fn disconnect(
    reason_override: Option<DisconnectReason>,
    packets_to_process_first: &[&[u8]],
    mut channel_handle: MutexGuard<Channel>,
    socket: &Arc<UdpSocket>,
    server_options: &Arc<ServerOptions>,
) {
    // Allow processing some packets first so we can add the session ID to the disconnect packet
    packets_to_process_first.iter().for_each(|packet| {
        if let Err(err) = channel_handle.receive(packet, server_options) {
            info!(
                "Couldn't deserialize packet while processing disconnect for client {}: {:?}",
                channel_handle.addr, err
            );
        }
    });
    channel_handle.process_all(server_options);

    let disconnect_reason = reason_override
        .or(channel_handle.disconnect_reason)
        .unwrap_or(DisconnectReason::Unknown);
    disconnect_or_log_err(&mut channel_handle, disconnect_reason, socket);
}

fn log_out_and_disconnect(
    reason_override: Option<DisconnectReason>,
    guid: u32,
    packets_to_process_first: &[&[u8]],
    channel: Mutex<Channel>,
    game_server: &Arc<GameServer>,
    socket: &Arc<UdpSocket>,
    server_options: &Arc<ServerOptions>,
) -> Vec<Broadcast> {
    disconnect(
        reason_override,
        packets_to_process_first,
        channel.lock(),
        socket,
        server_options,
    );
    match game_server.log_out(guid) {
        Ok(log_out_broadcasts) => log_out_broadcasts,
        Err(err) => {
            info!("Unable to log out existing player {}: {}", guid, err);
            Vec::new()
        }
    }
}
