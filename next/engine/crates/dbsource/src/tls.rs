//! TLS for the network drivers: rustls with the ring provider. `verify` checks the certificate against the computer's trusted
//! roots (Windows: the system store; macOS, Linux: the PEM bundle the OS keeps), or, with `trustFingerprint`, accepts exactly
//! that certificate (a self-signed server the user chose to trust). `require` encrypts without checking who answers.
//! The certificate the server showed is kept (`seen`) so a refusal can offer its fingerprint. A version 1 certificate (the one SQL Server
//! makes for itself) works where no roots are asked (`require`, a pinned fingerprint): its handshake signature is checked with its key alone.
use crate::error::{DbError, Kind, Result};
use crate::model::{Tls, TlsMode};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, WebPkiSupportedAlgorithms};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
use std::sync::{Arc, Mutex};

pub type Seen = Arc<Mutex<Option<String>>>;

/// SHA-256 of a certificate as `AB:CD:…`.
pub fn fingerprint(der: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(der).iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

fn normalize(fp: &str) -> String { fp.chars().filter(char::is_ascii_hexdigit).collect::<String>().to_ascii_uppercase() }

#[derive(Debug)]
struct Verifier {
    inner: Option<Arc<dyn ServerCertVerifier>>,
    pinned: Option<String>,
    algorithms: WebPkiSupportedAlgorithms,
    seen: Seen,
}

impl ServerCertVerifier for Verifier {
    fn verify_server_cert(&self, end: &CertificateDer<'_>, inter: &[CertificateDer<'_>], name: &ServerName<'_>, ocsp: &[u8], now: UnixTime) -> std::result::Result<ServerCertVerified, rustls::Error> {
        let fp = fingerprint(end.as_ref());
        if let Ok(mut s) = self.seen.lock() { *s = Some(fp.clone()); }
        if let Some(pinned) = &self.pinned {
            return if normalize(pinned) == normalize(&fp) { Ok(ServerCertVerified::assertion()) }
                else { Err(rustls::Error::General(format!("the server's certificate is not the trusted one (SHA-256 {fp})"))) };
        }
        match &self.inner { Some(inner) => inner.verify_server_cert(end, inter, name, ocsp, now), None => Ok(ServerCertVerified::assertion()) }
    }
    fn verify_tls12_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
            .or_else(|e| if self.inner.is_none() { raw_signature(message, cert, dss, &self.algorithms, false).map_err(|_| e) } else { Err(e) })
    }
    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
            .or_else(|e| if self.inner.is_none() { raw_signature(message, cert, dss, &self.algorithms, true).map_err(|_| e) } else { Err(e) })
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> { self.algorithms.supported_schemes() }
}

/// One DER element: (tag, contents, the rest after it).
fn der(b: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let (&tag, b) = b.split_first()?;
    let (&first, b) = b.split_first()?;
    let (len, b) = if first < 0x80 { (usize::from(first), b) } else {
        let n = usize::from(first & 0x7F);
        if n == 0 || n > 4 || b.len() < n { return None; }
        (b[..n].iter().fold(0usize, |a, &x| (a << 8) | usize::from(x)), &b[n..])
    };
    (b.len() >= len).then(|| (tag, &b[..len], &b[len..]))
}

/// The public key of a certificate (algorithm identifier contents, key bits), read straight from the DER. webpki reads only version 3
/// certificates; the certificate SQL Server makes for itself when none is set up is version 1.
fn public_key(cert: &[u8]) -> Option<(&[u8], &[u8])> {
    let (0x30, cert, _) = der(cert)? else { return None };
    let (0x30, tbs, _) = der(cert)? else { return None };
    let mut rest = tbs;
    if rest.first() == Some(&0xA0) { rest = der(rest)?.2; } // [0] version, absent in version 1
    for _ in 0..5 { rest = der(rest)?.2; } // serial, signature algorithm, issuer, validity, subject
    let (0x30, spki, _) = der(rest)? else { return None };
    let (0x30, alg, rest) = der(spki)? else { return None };
    let (0x03, bits, _) = der(rest)? else { return None };
    let (0, key) = bits.split_first()? else { return None };
    Some((alg, key))
}

/// The handshake signature checked with the certificate's public key alone, when webpki cannot read the certificate. Used only where the
/// certificate is not checked against roots (`require`, or a pinned fingerprint: then the certificate is exactly the pinned one, and this
/// proves the server holds its private key).
fn raw_signature(message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct, algorithms: &WebPkiSupportedAlgorithms, tls13: bool) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
    let bad = || rustls::Error::General("the server's handshake signature does not match its certificate".into());
    let (alg_id, key) = public_key(cert.as_ref()).ok_or_else(bad)?;
    let candidates = algorithms.mapping.iter().find(|(s, _)| *s == dss.scheme).map(|(_, a)| *a).ok_or_else(bad)?;
    let candidates = if tls13 { &candidates[..candidates.len().min(1)] } else { candidates };
    for alg in candidates {
        if alg.public_key_alg_id().as_ref() == alg_id && alg.verify_signature(key, message, dss.signature()).is_ok() {
            return Ok(HandshakeSignatureValid::assertion());
        }
    }
    Err(bad())
}

#[cfg(target_os = "windows")]
fn system_roots() -> Vec<CertificateDer<'static>> {
    use windows_sys::Win32::Security::Cryptography::{CertCloseStore, CertEnumCertificatesInStore, CertOpenSystemStoreW};
    let name: Vec<u16> = "ROOT".encode_utf16().chain(std::iter::once(0)).collect();
    let mut out = Vec::new();
    // SAFETY: the store is opened, walked with the API's own enumeration (each context is freed by the next call) and closed
    unsafe {
        let store = CertOpenSystemStoreW(0, name.as_ptr());
        if store.is_null() { return out; }
        let mut ctx = CertEnumCertificatesInStore(store, std::ptr::null());
        while !ctx.is_null() {
            let c = &*ctx;
            out.push(CertificateDer::from(std::slice::from_raw_parts(c.pbCertEncoded, c.cbCertEncoded as usize).to_vec()));
            ctx = CertEnumCertificatesInStore(store, ctx);
        }
        CertCloseStore(store, 0);
    }
    out
}

#[cfg(not(target_os = "windows"))]
fn system_roots() -> Vec<CertificateDer<'static>> {
    use rustls::pki_types::pem::PemObject;
    let mut files: Vec<std::path::PathBuf> = std::env::var_os("SSL_CERT_FILE").map(Into::into).into_iter().collect();
    files.extend(["/etc/ssl/cert.pem", "/etc/ssl/certs/ca-certificates.crt", "/etc/pki/tls/certs/ca-bundle.crt",
        "/etc/ssl/ca-bundle.pem", "/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem"].map(std::path::PathBuf::from));
    for f in files {
        if let Ok(bytes) = std::fs::read(&f) {
            let certs: Vec<_> = CertificateDer::pem_slice_iter(&bytes).filter_map(|c| c.ok()).collect();
            if !certs.is_empty() { return certs; }
        }
    }
    // Android keeps one PEM file per root (`<hash>.0`): the Conscrypt module's folder (Android 14 and later), else the system's
    // (dbplatform-c1; compiled, not run on a device). iOS gives no list of its roots: `verify_info` says so.
    if cfg!(target_os = "android") {
        for dir in ["/apex/com.android.conscrypt/cacerts", "/system/etc/security/cacerts"] {
            let Ok(entries) = std::fs::read_dir(dir) else { continue };
            let certs: Vec<_> = entries.filter_map(|e| std::fs::read(e.ok()?.path()).ok())
                .flat_map(|bytes| CertificateDer::pem_slice_iter(&bytes).filter_map(|c| c.ok()).collect::<Vec<_>>()).collect();
            if !certs.is_empty() { return certs; }
        }
    }
    Vec::new()
}

/// Whether `verify` (checking the server's certificate against the trusted roots) can work on this device: not on iOS, which gives
/// no list of its trusted roots (a pinned certificate, `trustFingerprint`, still works there).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyInfo {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<&'static str>,
}

pub fn verify_info() -> VerifyInfo {
    if cfg!(target_os = "ios") {
        return VerifyInfo { available: false, reason: Some("iosNoRootList"),
            message: Some("iOS gives no list of its trusted roots yet: trust the server's certificate by its fingerprint, or use require") };
    }
    VerifyInfo { available: true, reason: None, message: None }
}

/// The trusted roots as PEM text, for a driver that reads its roots from a file (MySQL/MariaDB): the same roots `verify` uses, only the
/// certificates a root store accepts (one that does not parse must not stop the connection).
pub fn roots_pem() -> Result<String> {
    let mut out = String::new();
    let mut n = 0;
    for der in system_roots() {
        if RootCertStore::empty().add(der.clone()).is_err() { continue; }
        out.push_str("-----BEGIN CERTIFICATE-----\n");
        let b64 = base64(der.as_ref());
        for chunk in b64.as_bytes().chunks(64) { out.push_str(std::str::from_utf8(chunk).unwrap_or_default()); out.push('\n'); }
        out.push_str("-----END CERTIFICATE-----\n");
        n += 1;
    }
    if n == 0 { return Err(DbError::new(Kind::Tls, "no trusted root certificates were found on this computer")); }
    Ok(out)
}

fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { A[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { A[n as usize & 63] as char } else { '=' });
    }
    out
}

/// MySQL and MariaDB start TLS inside their own protocol: the server's greeting, then a 32-byte "SSL request", then the TLS handshake. This does
/// those steps with the settings of `client_config` (the computer's roots, or the pinned fingerprint) and gives the SHA-256 of the certificate the
/// server showed. The driver cannot say which certificate it refused or check a pinned one, so the host asks here: a refusal can offer the
/// fingerprint ("이 인증서 신뢰"), and a pinned certificate is checked before the driver connects (design 0, TLS).
pub fn probe_mysql(host: &str, port: u16, tls: &Tls) -> Result<String> {
    use std::io::{Read, Write};
    use std::net::{TcpStream, ToSocketAddrs};
    let timeout = std::time::Duration::from_millis(crate::CONNECT_TIMEOUT_MS);
    let network = |e: std::io::Error| DbError::new(if matches!(e.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock) { Kind::Timeout } else { Kind::Network }, e.to_string());
    let addr = (host, port).to_socket_addrs().map_err(network)?.next().ok_or_else(|| DbError::new(Kind::Network, "the host has no address"))?;
    let mut tcp = TcpStream::connect_timeout(&addr, timeout).map_err(network)?;
    tcp.set_read_timeout(Some(timeout)).map_err(network)?;
    tcp.set_write_timeout(Some(timeout)).map_err(network)?;
    // the greeting (protocol 10): version, server version (ends with 0), connection id (4), scramble (8), filler (1), capability flags (2, low half)
    let mut head = [0u8; 4];
    tcp.read_exact(&mut head).map_err(network)?;
    let len = u32::from_le_bytes([head[0], head[1], head[2], 0]) as usize;
    let not_mysql = || DbError::new(Kind::Network, "the server did not greet like MySQL or MariaDB");
    if !(20..=4096).contains(&len) { return Err(not_mysql()); }
    let mut greeting = vec![0u8; len];
    tcp.read_exact(&mut greeting).map_err(network)?;
    if greeting[0] != 10 { return Err(not_mysql()); }
    let end = greeting.iter().skip(1).position(|&b| b == 0).map(|i| i + 1).ok_or_else(not_mysql)?;
    let at = end + 1 + 4 + 8 + 1;
    let caps = greeting.get(at..at + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]));
    if caps & 0x0800 == 0 { return Err(DbError::new(Kind::Tls, "the server does not offer TLS")); }
    // the SSL request: capabilities (long password, protocol 41, SSL, secure connection), max packet, utf8mb4, 23 zero bytes
    let mut request = vec![32, 0, 0, 1];
    request.extend((0x0001u32 | 0x0200 | 0x0800 | 0x8000).to_le_bytes());
    request.extend(0x0100_0000u32.to_le_bytes());
    request.push(45);
    request.extend([0u8; 23]);
    tcp.write_all(&request).map_err(network)?;
    let (config, seen) = client_config(tls)?;
    let name = ServerName::try_from(host.to_owned()).map_err(|e| DbError::new(Kind::Tls, e.to_string()))?;
    let mut conn = rustls::ClientConnection::new(Arc::new(config), name).map_err(|e| DbError::new(Kind::Tls, e.to_string()))?;
    let shown = || seen.lock().ok().and_then(|s| s.clone());
    while conn.is_handshaking() {
        if let Err(e) = conn.complete_io(&mut tcp) {
            let mut d = DbError::new(Kind::Tls, e.get_ref().map_or_else(|| e.to_string(), ToString::to_string));
            d.fingerprint = shown();
            return Err(d);
        }
    }
    shown().ok_or_else(|| DbError::new(Kind::Tls, "the server showed no certificate"))
}

/// The client configuration for a connection's TLS settings, and where the certificate the server shows is noted.
pub fn client_config(tls: &Tls) -> Result<(ClientConfig, Seen)> {
    let provider: Arc<CryptoProvider> = Arc::new(rustls::crypto::ring::default_provider());
    let algorithms = provider.signature_verification_algorithms;
    let seen: Seen = Arc::new(Mutex::new(None));
    let builder = ClientConfig::builder_with_provider(provider.clone()).with_safe_default_protocol_versions()
        .map_err(|e| DbError::new(Kind::Tls, e.to_string()))?;
    let pinned = tls.trust_fingerprint.clone().filter(|f| !f.trim().is_empty());
    let inner: Option<Arc<dyn ServerCertVerifier>> = match (tls.mode, &pinned) {
        (TlsMode::Verify, None) => {
            let mut roots = RootCertStore::empty();
            let (added, _) = roots.add_parsable_certificates(system_roots());
            if added == 0 { return Err(DbError::new(Kind::Tls, "no trusted root certificates were found on this computer")); }
            Some(rustls::client::WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider).build()
                .map_err(|e| DbError::new(Kind::Tls, e.to_string()))?)
        }
        _ => None,
    };
    let verifier = Arc::new(Verifier { inner, pinned, algorithms, seen: seen.clone() });
    Ok((builder.dangerous().with_custom_certificate_verifier(verifier).with_no_client_auth(), seen))
}

#[cfg(test)]
mod tests {
    #[test]
    fn fingerprints_compare_without_colons_or_case() {
        assert_eq!(super::normalize("ab:CD:01"), "ABCD01");
        assert_eq!(super::fingerprint(b"abc"), "BA:78:16:BF:8F:01:CF:EA:41:41:40:DE:5D:AE:22:23:B0:03:61:A3:96:17:7A:9C:B4:10:FF:61:F2:00:15:AD");
        assert!(!super::system_roots().is_empty(), "the computer's trusted roots are read");
    }

    #[test]
    fn the_public_key_is_read_from_a_version_1_certificate_too() {
        fn tlv(tag: u8, content: &[u8]) -> Vec<u8> {
            let mut out = vec![tag];
            if content.len() < 0x80 { out.push(content.len() as u8) } else { out.extend([0x82, (content.len() >> 8) as u8, content.len() as u8]) }
            out.extend_from_slice(content);
            out
        }
        let hex = |s: &str| (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect::<Vec<u8>>();
        // an EC P-256 key (made with openssl for this test)
        let spki = hex("3059301306072a8648ce3d020106082a8648ce3d03010703420004ec840771ea59fc7b019450154538f47d51146897ea4e1d6cd929e17d344c76f5d476814dbea3749bf862ac9df8b0b4b537c62862fc6d512f337f2276d3e3da45");
        let name = tlv(0x30, &tlv(0x31, &tlv(0x30, &[&hex("0603550403")[..], &tlv(0x0C, b"folio-test")[..]].concat())));
        let validity = tlv(0x30, &[tlv(0x17, b"261120000000Z"), tlv(0x17, b"361120000000Z")].concat());
        let alg = tlv(0x30, &hex("06082a8648ce3d040302"));
        for version in [None, Some(tlv(0xA0, &tlv(0x02, &[2])))] {
            let tbs = tlv(0x30, &[version.clone().unwrap_or_default(), tlv(0x02, &[1]), alg.clone(), name.clone(), validity.clone(), name.clone(), spki.clone(),
                if version.is_some() { tlv(0xA3, &tlv(0x30, &[])) } else { vec![] }].concat());
            let cert = tlv(0x30, &[tbs, alg.clone(), tlv(0x03, &[0, 1, 2, 3])].concat());
            let (alg_id, key) = super::public_key(&cert).expect("the key");
            assert_eq!(alg_id, &hex("06072a8648ce3d020106082a8648ce3d030107")[..], "id-ecPublicKey, prime256v1");
            assert_eq!(key, &spki[spki.len() - 65..], "the uncompressed point");
        }
        assert!(super::public_key(&[0x30, 0x05, 0x30, 0x03, 0x02, 0x01]).is_none(), "a cut certificate gives no key");
    }

    #[test]
    fn the_roots_are_given_to_a_driver_as_pem() {
        assert_eq!(super::base64(b""), "");
        assert_eq!(super::base64(b"f"), "Zg==");
        assert_eq!(super::base64(b"fo"), "Zm8=");
        assert_eq!(super::base64(b"foo"), "Zm9v");
        assert_eq!(super::base64(b"foobar"), "Zm9vYmFy");
        let pem = super::roots_pem().unwrap();
        assert!(pem.starts_with("-----BEGIN CERTIFICATE-----\n") && pem.trim_end().ends_with("-----END CERTIFICATE-----"));
        assert!(pem.lines().all(|l| l.len() <= 64 || l.starts_with("-----")), "the base64 lines are at most 64 characters");
    }
}
