use std::{any, cell::LazyCell, net::{IpAddr, Ipv6Addr}, path::PathBuf, ptr::addr_of_mut, sync::LazyLock, time::{Duration, Instant}};

use image::{self, GenericImageView, Rgba};
use clap::Parser;
use anyhow;
use pnet::packet::{icmpv6::{echo_request::{EchoRequestPacket, Icmpv6Codes}, Icmpv6Code, Icmpv6Types}, ip::{IpNextHeaderProtocol, IpNextHeaderProtocols::{self, Ipv6}}, util, Packet};
use rand::seq::SliceRandom;

static SNT_ASCII: &'static str = include_str!("snt.asc");

// center
// randomize location
// multithreading
// rotation

#[derive(clap::Parser)]
struct Args {
    #[arg(short, long, help="image file to ping")]
    file: PathBuf,
    #[arg(short, long, default_value="1", help="interval for printing stats")]
    interval: u64,
    #[arg(short, long, default_value="0", help="x offset")]
    x: u16,
    #[arg(short, long, default_value="0", help="y offset")]
    y: u16, 
    #[arg(short, long, default_value="false", help="only print once")]
    once: bool,
    #[arg(long, requires="scale_y")]
    scale_x: Option<u32>,
    #[arg(long, requires="scale_x")]
    scale_y: Option<u32>,
    #[arg(long, default_value="true", help="shuffle pixel order")]
    shuffle: bool,
    #[arg(short, long, default_value="false", help="don't print anything")]
    quiet: bool,
    #[arg(short, long, help="ping this image once every x seconds")]
    timeout: Option<u64>,
    #[arg(long, default_value="false", help="don't send translucent pixels")]
    transparent: bool,
    #[arg(long, help="how many degrees to rotate hue by every 5 seconds")]
    huerotatespeed: Option<i32>
}

static DESTADDR: (u16, u16, u16, u16) = (0x2001, 0x610, 0x1908, 0xa000);

#[inline]
fn craftaddr(x: u16, y: u16, r: u8, g: u8, b: u8) -> IpAddr {
    std::net::IpAddr::V6(Ipv6Addr::new(
        DESTADDR.0,
        DESTADDR.1,
        DESTADDR.2,
        DESTADDR.3,
        x,
        y,
        ((b as u16) << 8) | g as u16,
        ((r as u16) << 8) | 0xff,
    ))
}

// sysctl net.core.rmem_default=16777216
// sysctl net.core.wmem_default=16777216
// sysctl net.core.rmem_max=16777216
// sysctl net.core.wmem_max=16777216

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    //if !args.quiet { println!("{SNT_ASCII}") }
    let mut image = image::open(args.file)?;

    if args.scale_x.is_some() {
        image = image.resize_exact(
            args.scale_x.unwrap(),
            args.scale_y.unwrap(),
            image::imageops::FilterType::Gaussian
        );
    }

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

    let mut stattimer = Instant::now();
    let interval = Duration::new(args.interval, 0);

    let mut timeouttimer = Instant::now();
    let timeout = args.timeout.map(|t| Duration::new(t,0));

    let mut huetimer = Instant::now();
    let huerotatetimeout = Duration::new(5, 0);

    let mut count_total = 0;
    let mut countpks = 0;
    let mut errors_total = 0;
    let mut errorspks = 0;

    let ogimage = image.clone();

    let mut pixels: Vec<(u32, u32, Rgba<u8>)> = image.pixels().filter(
        |(_,_,Rgba([_,_,_,a]))| !args.transparent || *a == 0xff
    ).collect();
    if args.shuffle {
        pixels.shuffle(&mut rand::thread_rng());
    }

    let mut hue = 0;

    loop {
        if let Some(timeout) = timeout {
            if timeouttimer.elapsed() > timeout {
                if !args.quiet { println!("Done sleeping") }
                timeouttimer = Instant::now();
            } else {
                std::thread::sleep(Duration::new(1,0));
                continue;
            }
        }
        if let Some(huerotatespeed) = args.huerotatespeed {
            if huetimer.elapsed() > huerotatetimeout {
                huetimer = Instant::now();
                hue = (hue + huerotatespeed) % 360;
                if !args.quiet { println!("rotating hue by {huerotatespeed}: {hue}") }
                image = ogimage.huerotate(hue);
                pixels = image.pixels().filter(
                    |(_,_,Rgba([_,_,_,a]))| !args.transparent || *a == 0xff
                ).collect();
                if args.shuffle { pixels.shuffle(&mut rand::thread_rng()) }
            }
        }
        for (x, y, Rgba([r, g, b, _a])) in &pixels {
            if let Err(_) = tx.send_to(&echoreq, craftaddr(args.x + *x as u16, args.y + *y as u16 , *r, *g, *b)) {
                errors_total += 1;
                errorspks += 1;
            }
            countpks += 1;
            count_total += 1;
            if stattimer.elapsed() > interval && !args.quiet {
                stattimer = Instant::now();
                println!("{countpks}pks {errorspks}eps ({:02.0}%) {count_total} packets {errors_total} errors", 100.0 * errorspks as f64 / countpks as f64);
                errorspks = 0;
                countpks = 0;
            }
        }
        if args.once {
            break;
        }
    }

    Ok(())
}
