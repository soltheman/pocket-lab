//! `iw4l-master lan`: a co-op relay for one Wi-Fi network, started with no setup.
//!
//! The first run makes a private certificate authority and a server certificate for
//! the fixed name [`SERVER_NAME`] and keeps them in the folder. Every run writes the
//! `.iw4l-server` file that game installs need (this PC's Wi-Fi address and that
//! authority), then serves on all interfaces. The certificate names no address, so it
//! survives the PC getting a new address; only the `.iw4l-server` file changes.

use std::fs;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};

use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair,
    KeyUsagePurpose,
};

use crate::Result;

pub const SERVER_NAME: &str = "pocket-lab-lan";
pub const DEFAULT_PORT: u16 = 4433;
const DESCRIPTOR: &str = "Pocket Lab Co-op.iw4l-server";

pub struct Lan {
    pub bind: SocketAddr,
    pub cert: PathBuf,
    pub key: PathBuf,
}

/// Makes or reuses the certificates in `dir`, writes the descriptor for `address`
/// (this PC's Wi-Fi address when `None`), and says what to do with it.
pub fn prepare(dir: &Path, port: u16, address: Option<IpAddr>) -> Result<Lan> {
    fs::create_dir_all(dir)?;
    let ca_pem = dir.join("ca.pem");
    let cert = dir.join("server.pem");
    let key = dir.join("server.key");
    if !(ca_pem.is_file() && cert.is_file() && key.is_file()) {
        issue(dir)?;
    }
    let ip = match address {
        Some(ip) => ip,
        None => lan_address()?,
    };
    let ca = fs::read_to_string(&ca_pem)?;
    let descriptor = dir.join(DESCRIPTOR);
    let address = SocketAddr::new(ip, port);
    fs::write(
        &descriptor,
        format!(
            "schema = 1\nname = \"Pocket Lab Co-op\"\n[master]\naddress = \"{address}\"\nserver_name = \"{SERVER_NAME}\"\n[updates]\nurl = \"\"\nca_pem = \"\"\"\n{}\n\"\"\"\n",
            ca.trim_end()
        ),
    )?;
    println!("Co-op server for this Wi-Fi network: {ip}:{port}");
    println!();
    println!("Copy this file into the game on every device that plays:");
    println!("  {}", descriptor.display());
    println!("  iPhone: Files app > On My iPhone > Pocket Lab (next to the Games folder)");
    println!("  PC: next to iw4l.exe");
    println!();
    println!("Every device must be on this Wi-Fi network. If Windows asks, allow");
    println!("iw4l-master on private networks. Leave this window open while you play.");
    Ok(Lan {
        bind: SocketAddr::new(IpAddr::from([0, 0, 0, 0]), port),
        cert,
        key,
    })
}

/// A private authority and a server certificate it signs for [`SERVER_NAME`].
fn issue(dir: &Path) -> Result<()> {
    let ca_key = KeyPair::generate()?;
    let mut ca = CertificateParams::new(Vec::<String>::new())?;
    ca.distinguished_name
        .push(DnType::CommonName, "Pocket Lab co-op authority");
    ca.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    let ca_cert = ca.self_signed(&ca_key)?;
    let server_key = KeyPair::generate()?;
    let mut server = CertificateParams::new(vec![SERVER_NAME.to_owned()])?;
    server.distinguished_name.push(DnType::CommonName, SERVER_NAME);
    server.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let server_cert = server.signed_by(&server_key, &ca_cert, &ca_key)?;
    fs::write(dir.join("ca.pem"), ca_cert.pem())?;
    fs::write(dir.join("server.pem"), server_cert.pem())?;
    fs::write(dir.join("server.key"), server_key.serialize_pem())?;
    Ok(())
}

/// The address this PC uses on its local network: the source address of a route to a
/// private address. Connecting a UDP socket sends nothing.
fn lan_address() -> Result<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.connect("10.255.255.255:1")?;
    let ip = socket.local_addr()?.ip();
    if ip.is_loopback() || ip.is_unspecified() {
        return Err("no Wi-Fi or network address found; connect to a network, or pass --address".into());
    }
    Ok(ip)
}
