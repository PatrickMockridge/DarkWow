/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * DarkWow is a tool for people and nations to establish sovereignty
 * according to human rights law. See the UN Declaration on the Rights
 * of Indigenous Peoples and associated documents:
 * https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{io, sync::{Arc, OnceLock}};

/// HAZOP H1: SSH-style known_hosts for TLS certificate pinning.
///
/// Maps hostname → blake3(cert.der) and persists it to the P2P datastore. This was an in-memory
/// `HashMap` with no constructor anywhere in the P2P stack, so `localnet = false` validated a
/// certificate's SAN and then accepted any certificate bearing the shared `dark.fi` SAN — which
/// anyone can mint, because the nodes self-sign. The pin is what authenticates a peer.
pub(crate) type TrustStore = Arc<crate::net::known_hosts::KnownHosts>;

use futures_rustls::{
    rustls::{
        self,
        client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
        pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime},
        server::danger::{ClientCertVerified, ClientCertVerifier},
        version::TLS13,
        ClientConfig, DigitallySignedStruct, DistinguishedName, ServerConfig, SignatureScheme,
    },
    TlsAcceptor, TlsConnector, TlsStream,
};
use rcgen::string::Ia5String;
use tracing::{error, warn};
use x509_parser::{
    parse_x509_certificate,
    prelude::{GeneralName, ParsedExtension, X509Certificate},
};

/// The DNS name used for certificate validation across all transports
pub(crate) const TLS_DNS_NAME: &str = "dark.fi";

/// Validate certificate DNSName.
fn validate_dnsname(cert: &X509Certificate) -> std::result::Result<(), rustls::Error> {
    #[rustfmt::skip]
    let oid = x509_parser::oid_registry::asn1_rs::oid!(2.5.29.17);
    let Ok(Some(extension)) = cert.get_extension_unique(&oid) else {
        return Err(rustls::CertificateError::BadEncoding.into())
    };

    let dns_name = match extension.parsed_extension() {
        ParsedExtension::SubjectAlternativeName(altname) => {
            if altname.general_names.len() != 1 {
                return Err(rustls::CertificateError::BadEncoding.into())
            }

            match altname.general_names[0] {
                GeneralName::DNSName(dns_name) => dns_name,
                _ => return Err(rustls::CertificateError::BadEncoding.into()),
            }
        }

        _ => return Err(rustls::CertificateError::BadEncoding.into()),
    };

    if dns_name != TLS_DNS_NAME {
        return Err(rustls::CertificateError::BadEncoding.into())
    }

    Ok(())
}

fn verify_ed25519_signature(
    message: &[u8],
    cert: &CertificateDer,
    dss: &DigitallySignedStruct,
) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
    if dss.scheme != SignatureScheme::ED25519 {
        return Err(rustls::CertificateError::BadSignature.into())
    }

    // Read the DER-encoded certificate into a buffer
    let buf: Vec<u8> = cert.iter().copied().collect();

    // Parse the cert and extract the public key
    let Ok((_, cert)) = parse_x509_certificate(&buf) else {
        error!(target: "net::tls::verify_ed25519_signature", "[net::tls] Failed parsing TLS certificate");
        return Err(rustls::CertificateError::BadEncoding.into())
    };

    let Ok(public_key) = ed25519_compact::PublicKey::from_der(cert.public_key().raw) else {
        error!(target: "net::tls::verify_ed25519_signature", "[net::tls] Failed parsing public key");
        return Err(rustls::CertificateError::BadEncoding.into())
    };

    let Ok(signature) = ed25519_compact::Signature::from_slice(dss.signature()) else {
        error!(target: "net::tls::verify_ed25519_signature", "[net::tls] Failed verifying signature");
        return Err(rustls::CertificateError::BadSignature.into())
    };

    if let Err(e) = public_key.verify(message, &signature) {
        error!(target: "net::tls::verify_ed25519_signature", "[net::tls] Failed verifying signature: {e}");
        return Err(rustls::CertificateError::BadSignature.into())
    }

    Ok(HandshakeSignatureValid::assertion())
}

#[derive(Debug)]
pub(crate) struct ServerCertificateVerifier {
    localnet: bool,
    trust_store: Option<TrustStore>,
}

impl ServerCertificateVerifier {
    pub fn new(localnet: bool, trust_store: Option<TrustStore>) -> Self {
        Self { localnet, trust_store }
    }
}

impl ServerCertVerifier for ServerCertificateVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer,
        _intermediates: &[CertificateDer],
        server_name: &ServerName,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        // Read the DER-encoded certificate into a buffer
        let buf: Vec<u8> = end_entity.iter().copied().collect();

        // Parse the certificate
        let Ok((_, cert)) = parse_x509_certificate(&buf) else {
            error!(target: "net::tls::verify_server_cert", "[net::tls] Failed parsing server TLS certificate");
            return Err(rustls::CertificateError::BadEncoding.into())
        };

        // Validate DNSName only when not in localnet mode
        if !self.localnet {
            validate_dnsname(&cert)?;
        } else {
            tracing::debug!(target: "net::tls", "Localnet mode: skipping DNS name validation");
        }

        // HAZOP H1: TOFU certificate pinning, **per peer host**.
        //
        // This used to key the store by the constant `TLS_DNS_NAME`, so every peer shared one
        // fingerprint: the first public peer to hand out a certificate pinned it for all of them,
        // and the second peer — with its own self-signed certificate — was rejected as a changed
        // host. The SAN stays a domain-wide tag (all nodes self-sign with the same SAN; see
        // `generate_certificate`), and identity is the fingerprint, so the pin must be per host.
        if !self.localnet {
            if let Some(ref store) = self.trust_store {
                let fp = *blake3::hash(end_entity.as_ref()).as_bytes();
                let hostname = match server_name {
                    ServerName::DnsName(dns) => dns.as_ref().to_string(),
                    // `rustls_pki_types::IpAddr` has no Display; its Debug form is stable and
                    // unique per address, which is all a trust-store key needs.
                    ServerName::IpAddress(ip) => format!("{ip:?}"),
                    _ => return Err(rustls::CertificateError::BadEncoding.into()),
                };
                if let Some(stored) = store.lookup(&hostname) {
                    if stored != fp {
                        error!(target: "net::tls::verify_server_cert",
                            "HOST IDENTIFICATION HAS CHANGED for {}: cert fingerprint mismatch", hostname);
                        return Err(rustls::CertificateError::UnknownIssuer.into())
                    }
                } else {
                    store.store(&hostname, fp);
                    if let Err(e) = store.save() {
                        // A pin that is not persisted is re-pinned on the next process start,
                        // which is exactly the window TOFU is meant to close. Keep the
                        // connection (the pin is enforced for this process), but say so.
                        warn!(target: "net::tls::verify_server_cert",
                            "TOFU: pinned {} but could not persist known_hosts: {e}", hostname);
                    }
                    tracing::info!(target: "net::tls::verify_server_cert",
                        "TOFU: stored initial cert fingerprint for {}", hostname);
                }
            }
        }

        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        unreachable!()
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        verify_ed25519_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }
}

#[derive(Debug)]
pub(crate) struct ClientCertificateVerifier {
    localnet: bool,
}

impl ClientCertificateVerifier {
    pub fn new(localnet: bool) -> Self {
        Self { localnet }
    }
}

impl ClientCertVerifier for ClientCertificateVerifier {
    fn offer_client_auth(&self) -> bool {
        true
    }

    fn client_auth_mandatory(&self) -> bool {
        true
    }

    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer,
        _intermediates: &[CertificateDer],
        _now: UnixTime,
    ) -> std::result::Result<ClientCertVerified, rustls::Error> {
        // Read the DER-encoded certificate into a buffer
        let buf: Vec<u8> = end_entity.iter().copied().collect();

        // Parse the certificate
        let Ok((_, cert)) = parse_x509_certificate(&buf) else {
            error!(target: "net::tls::verify_server_cert", "[net::tls] Failed parsing server TLS certificate");
            return Err(rustls::CertificateError::BadEncoding.into())
        };

        // Validate DNSName only when not in localnet mode
        if !self.localnet {
            validate_dnsname(&cert)?;
        } else {
            tracing::debug!(target: "net::tls", "Localnet mode: skipping DNS name validation");
        }

        // No pin here, deliberately. The store is keyed by the **server** host a client dials;
        // an inbound client presents no name rustls hands this callback, so the only thing a
        // "pin" could key on is the cert's own fingerprint — which can never mismatch. Pinning
        // every client to the first one seen is worse than not pinning: the first peer to
        // connect would lock every other peer out. Client certificates in this network are
        // self-signed within the `dark.fi` SAN domain, so they are a membership tag, not an
        // enrolled identity; the handshake signature still proves possession of the presented
        // key. Server authentication is where the TOFU pin belongs, and that is where it is.
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        unreachable!()
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        verify_ed25519_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }
}

/// HAZOP H1: process-lifetime certificate cache.
static CACHED_CERT: OnceLock<(CertificateDer<'static>, PrivateKeyDer<'static>)> = OnceLock::new();

/// Generate a self-signed Ed25519 certificate for TLS.
/// Cached per process lifetime for TOFU certificate pinning.
pub(crate) fn generate_certificate() -> io::Result<(CertificateDer<'static>, PrivateKeyDer<'static>)>
{
    if let Some(cert) = CACHED_CERT.get() {
        return Ok((cert.0.clone(), cert.1.clone_key()));
    }
    let Ok(keypair) = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519) else {
        return Err(io::Error::other("Failed to generate TLS keypair"))
    };

    let Ok(mut cert_params) = rcgen::CertificateParams::new(&[]) else {
        return Err(io::Error::other("Failed to generate TLS params"))
    };

    #[expect(clippy::unwrap_used, reason = "TLS_DNS_NAME is a valid IA5 string")]
    let san = rcgen::SanType::DnsName(Ia5String::try_from(TLS_DNS_NAME).unwrap());
    cert_params.subject_alt_names = vec![san];
    cert_params.extended_key_usages = vec![
        rcgen::ExtendedKeyUsagePurpose::ClientAuth,
        rcgen::ExtendedKeyUsagePurpose::ServerAuth,
    ];

    let Ok(certificate) = cert_params.self_signed(&keypair) else {
        return Err(io::Error::other("Failed to sign TLS certificate"))
    };

    let certificate = certificate.der().clone();
    let keypair_der = keypair.serialize_der();

    let Ok(secret_key_der) = PrivateKeyDer::try_from(keypair_der) else {
        return Err(io::Error::other("Failed to deserialize DER TLS secret"))
    };

    let result = (certificate, secret_key_der);
    let _ = CACHED_CERT.set((result.0.clone(), result.1.clone_key()));
    Ok(result)
}

pub struct TlsUpgrade {
    /// TLS server configuration
    server_config: Arc<ServerConfig>,
    /// TLS client configuration
    client_config: Arc<ClientConfig>,
}

impl TlsUpgrade {
    pub async fn new(localnet: bool, trust_store: Option<TrustStore>) -> io::Result<Self> {
        // Generate keypair and certificate (cached per process lifetime)
        let (certificate, secret_key_der) = generate_certificate()?;

        // Server-side config with localnet flag
        let client_cert_verifier = Arc::new(ClientCertificateVerifier::new(localnet));
        #[expect(clippy::unwrap_used, reason = "self-signed cert/key are compatible")]
        let server_config = Arc::new(
            ServerConfig::builder_with_protocol_versions(&[&TLS13])
                .with_client_cert_verifier(client_cert_verifier)
                .with_single_cert(vec![certificate.clone()], secret_key_der.clone_key())
                .unwrap(),
        );

        // Client-side config with localnet flag
        let server_cert_verifier = Arc::new(ServerCertificateVerifier::new(localnet, trust_store));
        #[expect(clippy::unwrap_used, reason = "self-signed cert/key are compatible")]
        let client_config = Arc::new(
            ClientConfig::builder_with_protocol_versions(&[&TLS13])
                .dangerous()
                .with_custom_certificate_verifier(server_cert_verifier)
                .with_client_auth_cert(vec![certificate.clone()], secret_key_der)
                .unwrap(),
        );

        Ok(Self { server_config, client_config })
    }

    /// Upgrade an outbound stream to TLS.
    ///
    /// `peer_host` is the host the caller is dialing. It becomes the TLS server name for SNI and
    /// — the reason it is a parameter rather than the constant — the key under which the
    /// certificate is TOFU-pinned. A caller that omits it would give every peer one shared pin,
    /// which is the defect this signature exists to make impossible.
    pub async fn upgrade_dialer_tls<IO>(self, stream: IO, peer_host: &str) -> io::Result<TlsStream<IO>>
    where
        IO: super::PtStream,
    {
        let server_name = ServerName::try_from(peer_host.to_string())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid TLS peer host"))?;
        let connector = TlsConnector::from(self.client_config);
        let stream = match connector.connect(server_name, stream).await {
            Ok(s) => s,
            Err(e) => {
                warn!(target: "net::tls::upgrade_dialer_tls", "TLS handshake failed: {e}");
                return Err(e)
            }
        };
        Ok(TlsStream::Client(stream))
    }

    // TODO: Try to find a transparent way for this instead of implementing
    // the function separately for every transport type.
    pub async fn upgrade_listener_tcp_tls(
        self,
        listener: smol::net::TcpListener,
    ) -> io::Result<(TlsAcceptor, smol::net::TcpListener)> {
        Ok((TlsAcceptor::from(self.server_config), listener))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A self-signed certificate for `san`, the shape every DarkWow node generates.
    fn cert_with_san(san: &str) -> CertificateDer<'static> {
        let keypair = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519)
            .expect("generate a test keypair");
        let mut params = rcgen::CertificateParams::new(Vec::<String>::new())
            .expect("certificate params");
        params.subject_alt_names = vec![rcgen::SanType::DnsName(
            Ia5String::try_from(san).expect("test SAN is a valid IA5 string"),
        )];
        params.extended_key_usages = vec![
            rcgen::ExtendedKeyUsagePurpose::ClientAuth,
            rcgen::ExtendedKeyUsagePurpose::ServerAuth,
        ];
        params.self_signed(&keypair).expect("self-sign the test certificate").der().clone()
    }

    fn verify(
        verifier: &ServerCertificateVerifier,
        cert: &CertificateDer<'_>,
        host: &str,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        let name = ServerName::try_from(host.to_string()).expect("valid test server name");
        verifier.verify_server_cert(cert, &[], &name, &[], UnixTime::now())
    }

    fn verifier_at(path: std::path::PathBuf) -> ServerCertificateVerifier {
        let store = crate::net::known_hosts::KnownHosts::load(path)
            .expect("load an empty known_hosts store");
        ServerCertificateVerifier::new(false, Some(Arc::new(store)))
    }

    fn verifier() -> (ServerCertificateVerifier, std::path::PathBuf) {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "dwow_known_hosts_test_{}_{}.tsv",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        let _ = std::fs::remove_file(&path);
        (verifier_at(path.clone()), path)
    }

    /// `OBL-C247`: the SAN check refuses a certificate outside the DarkWow trust domain.
    #[test]
    fn a_certificate_for_another_domain_is_refused() {
        let (v, _path) = verifier();
        let cert = cert_with_san("attacker.example");
        assert!(verify(&v, &cert, "peer.example").is_err(), "a wrong-SAN certificate must be refused");
    }

    /// A certificate in the trust domain is accepted once, then pinned **for that host**.
    #[test]
    fn a_certificate_is_accepted_then_pinned_for_its_peer() {
        let (v, _path) = verifier();
        let cert = cert_with_san(TLS_DNS_NAME);
        verify(&v, &cert, "peer-a.example").expect("a DarkWow-domain certificate is admitted");

        // A different certificate presented by the same host is a changed identity.
        let replacement = cert_with_san(TLS_DNS_NAME);
        assert!(
            verify(&v, &replacement, "peer-a.example").is_err(),
            "a pinned peer presenting a different certificate must be refused"
        );

        // A *different* peer with its own certificate is not affected by peer A's pin. This is
        // the half that a trust store keyed by the constant got wrong: every peer shared one pin.
        verify(&v, &replacement, "peer-b.example")
            .expect("a second peer must be pinned independently");
    }

    /// TOFU means first use, not first use per process: the pin is persisted and a *new* verifier
    /// loading the same datastore enforces it.
    #[test]
    fn a_pin_is_persisted_and_enforced_by_a_fresh_verifier() {
        let (v, path) = verifier();
        let cert = cert_with_san(TLS_DNS_NAME);
        verify(&v, &cert, "peer-a.example").expect("first use pins the certificate");

        let fresh = verifier_at(path);
        let replacement = cert_with_san(TLS_DNS_NAME);
        assert!(
            verify(&fresh, &replacement, "peer-a.example").is_err(),
            "a fresh verifier must load the pin and refuse a changed certificate"
        );
    }
}
