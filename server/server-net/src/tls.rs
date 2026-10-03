//! TLS setup: certificate and key from PEM files, or a
//! self-signed pair generated once with `rcgen` and reused afterwards.
//! rustls always uses the `ring` provider.

use crate::NetError;
use rustls::ServerConfig;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::info;

pub struct TlsFiles {
    pub cert: PathBuf,
    pub key: PathBuf,
}

impl TlsFiles {
    /// Loads both files; if neither exists, generates a self-signed
    /// certificate for `names` (host names or IPs) and writes them.
    pub fn load_or_generate(&self, names: &[String]) -> Result<Arc<ServerConfig>, NetError> {
        match (self.cert.exists(), self.key.exists()) {
            (true, true) => {}
            (false, false) => self.generate(names)?,
            _ => {
                return Err(NetError::Tls(format!(
                    "only one of {} and {} exists; give both or neither",
                    self.cert.display(),
                    self.key.display()
                )));
            }
        }
        let certs = CertificateDer::pem_file_iter(&self.cert)
            .and_then(|it| it.collect::<Result<Vec<_>, _>>())
            .map_err(|e| NetError::Tls(format!("{}: {e}", self.cert.display())))?;
        let key = PrivateKeyDer::from_pem_file(&self.key)
            .map_err(|e| NetError::Tls(format!("{}: {e}", self.key.display())))?;
        server_config(certs, key)
    }

    fn generate(&self, names: &[String]) -> Result<(), NetError> {
        let cert = rcgen::generate_simple_self_signed(names.to_vec())
            .map_err(|e| NetError::Tls(format!("cannot generate a certificate: {e}")))?;
        write_file(&self.cert, cert.cert.pem().as_bytes(), false)?;
        write_file(&self.key, cert.signing_key.serialize_pem().as_bytes(), true)?;
        info!(cert = %self.cert.display(), ?names, "generated a self-signed TLS certificate; clients need --trust-cert with this file");
        Ok(())
    }
}

/// Server config from parsed certificate chain and key.
pub fn server_config(
    certs: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> Result<Arc<ServerConfig>, NetError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .and_then(|b| b.with_no_client_auth().with_single_cert(certs, key))
        .map(Arc::new)
        .map_err(|e| NetError::Tls(e.to_string()))
}

fn write_file(path: &Path, bytes: &[u8], secret: bool) -> Result<(), NetError> {
    let err = |e: std::io::Error| NetError::Tls(format!("{}: {e}", path.display()));
    if let Some(dir) = path.parent()
        && !dir.as_os_str().is_empty()
    {
        std::fs::create_dir_all(dir).map_err(err)?;
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    if secret {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    #[cfg(not(unix))]
    let _ = secret;
    use std::io::Write;
    opts.open(path).and_then(|mut f| f.write_all(bytes)).map_err(err)
}
