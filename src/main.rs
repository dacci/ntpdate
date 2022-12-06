mod ntp;

use anyhow::{bail, Result};
use bytes::BytesMut;
use clap::Parser;
use ntp::{Leap, Mode, Packet};
use std::net::SocketAddr;
use time::{format_description::well_known::Iso8601, OffsetDateTime};
use tokio::io;
use tokio::net::{lookup_host, UdpSocket};
use tokio::time::{timeout, Duration};

fn main() -> Result<()> {
    use tracing_subscriber::prelude::*;

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(tracing_subscriber::filter::LevelFilter::INFO.into())
                .from_env()
                .unwrap(),
        )
        .init();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main())
}

#[derive(Parser)]
#[command(about)]
struct Args {
    /// Host name or IP address of the server.
    #[arg(required = true)]
    server: Vec<String>,

    /// Specify the NTP version to send, which can be 1, 2, 3 or 4.
    #[arg(short = 'o', value_name = "version", default_value_t = 4)]
    version: u8,

    /// Specify the maximum time waiting for a server response, in seconds and fraction.
    #[arg(short, value_name = "seconds", default_value_t = 2.0)]
    timeout: f64,
}

async fn async_main() -> Result<()> {
    let args = Args::parse();
    if !(1..=4).contains(&args.version) {
        bail!("illegal NTP version `{}`", args.version);
    }

    let mut first = true;
    for server in &args.server {
        if first {
            first = false;
        } else {
            println!();
        }
        println!("{server}");

        let addrs = match lookup_host((server.as_str(), 123u16)).await {
            Ok(addrs) => addrs,
            Err(e) => {
                println!("  {e}");
                continue;
            }
        };

        let mut first = true;
        for addr in addrs {
            if first {
                first = false;
            } else {
                println!();
            }
            println!("  {}", addr.ip());

            if let Err(e) = access(&args, addr).await {
                println!("    {e}");
            }
        }
    }

    Ok(())
}

async fn access(args: &Args, addr: SocketAddr) -> Result<()> {
    let req = {
        let packet = Packet::new(Leap::NotInSync, 4, Mode::Client);
        let mut buf = BytesMut::with_capacity(48);
        packet.to_buf(&mut buf);
        buf.freeze()
    };

    let socket = UdpSocket::bind(if addr.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    })
    .await?;
    socket.connect(addr).await?;

    let len = socket.send(&req).await?;
    if len != req.len() {
        bail!("failed to send request");
    }

    let mut buf = BytesMut::with_capacity(1024);
    let len = loop {
        timeout(Duration::from_secs_f64(args.timeout), socket.readable()).await??;

        match socket.try_recv_buf(&mut buf) {
            Ok(len) => break len,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
            Err(e) => return Err(e.into()),
        };
    };
    if len < 48 {
        bail!("response too short");
    }

    let mut buf = buf.freeze();
    let packet = Packet::from_buf(&mut buf);

    let (leap, version, mode) = packet.leap_version_mode();
    let ref_id = if 1 < packet.stratum {
        format!(
            "{}.{}.{}.{}",
            packet.ref_id[0], packet.ref_id[1], packet.ref_id[2], packet.ref_id[3],
        )
    } else {
        packet
            .ref_id
            .map(|b| b as char)
            .into_iter()
            .collect::<String>()
    };

    println!(
        "    leap: {leap}, version: {version}, mode: {mode}, stratum: {}",
        packet.stratum,
    );
    println!("    poll: {}, precision: {}", packet.poll, packet.precision);
    println!(
        "    root delay: {:.9} seconds, root dispersion: {:.9} seconds",
        f64::from(packet.root_delay),
        f64::from(packet.root_dispersion),
    );
    println!("    reference id: {ref_id}");
    println!(
        "    reference timestamp: {}",
        OffsetDateTime::from(packet.reference_time)
            .format(&Iso8601::DEFAULT)
            .unwrap(),
    );
    println!(
        "      receive timestamp: {}",
        OffsetDateTime::from(packet.receive_time)
            .format(&Iso8601::DEFAULT)
            .unwrap(),
    );
    println!(
        "     transmit timestamp: {}",
        OffsetDateTime::from(packet.transmit_time)
            .format(&Iso8601::DEFAULT)
            .unwrap(),
    );

    Ok(())
}
