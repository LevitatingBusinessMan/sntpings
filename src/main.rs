use std::{any, cell::LazyCell, net::{IpAddr, Ipv6Addr}, path::PathBuf, ptr::addr_of_mut, sync::LazyLock, time::{Duration, Instant}};

use image;
use clap::{self, Parser};
use anyhow;
use pnet::packet::{icmpv6::{echo_request::{EchoRequestPacket, Icmpv6Codes}, Icmpv6Code, Icmpv6Types}, ip::{IpNextHeaderProtocol, IpNextHeaderProtocols::{self, Ipv6}}, util, Packet};
use rand::random;

#[derive(clap::Parser)]
struct Args {
    #[arg(short, long)]
    file: PathBuf,
    #[arg(short, long, default_value="1")]
    interval: u64,
}

static DESTADDR: (u16, u16, u16, u16) = (0x2001, 0x610, 0x1908, 0xa000);

fn craftaddr(x: u16, y: u16, r: u8, g: u8, b: u8) -> IpAddr {
    std::net::IpAddr::V6(Ipv6Addr::new(
        DESTADDR.0,
        DESTADDR.1,
        DESTADDR.2,
        DESTADDR.3,
        x,
        y,
        ((b as u16) << 8) | g as u16,
        ((r as u16) << 8) | 0,
    ))
}

// sysctl net.core.rmem_default=16777216
// sysctl net.core.wmem_default=16777216
// sysctl net.core.rmem_max=16777216
// sysctl net.core.wmem_max=16777216

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let image = image::open(args.file)?;

    use pnet::transport::TransportChannelType::Layer4;
    use pnet::transport::TransportProtocol::Ipv6;
    use pnet::packet::ip::IpNextHeaderProtocols;
    let protocol = Layer4(Ipv6(IpNextHeaderProtocols::Icmpv6));

    let (mut tx, _) = pnet::transport::transport_channel(4096, protocol)?;

    // https://www.rfc-editor.org/rfc/rfc4443#section-4.1
    let mut buffer = [0_u8; 64];
    let mut echoreq = pnet::packet::icmpv6::echo_request::MutableEchoRequestPacket::new(&mut buffer).unwrap();
    echoreq.set_icmpv6_code(Icmpv6Codes::NoCode);
    echoreq.set_icmpv6_type(Icmpv6Types::EchoRequest);
    echoreq.set_identifier(0);
    echoreq.set_sequence_number(0);
    let checksum = util::checksum(echoreq.packet(), 1);
    echoreq.set_checksum(checksum);
    let echoreq = echoreq.consume_to_immutable();

    let mut x = 0;
    let mut y = 0;
    let mut timer = Instant::now();
    let interval = Duration::new(args.interval, 0);

    let mut count_total = 0;
    let mut countpks = 0;
    let mut errors_total = 0;
    let mut errorspks = 0;
    loop {
        if let Err(_) = tx.send_to(&echoreq, craftaddr(1420 + x,30 + y,0,0,0xff)) {
            errors += 1;
            errorspks += 1;
        }
        countpks += 1;
        count_total += 1;
        x += 1;
        if x > 10 {
            x = 0;
            y += 1;
        }
        if y > 10 {
            y = 0;
        }
        if timer.elapsed() > interval {
            timer = Instant::now();
            println!("{countpks}pks {errorspks}eps {count_total} packets {errors_total} errors");
            errorspks = 0;
            countpks = 0;
        }
    }
    
    Ok(())
}
