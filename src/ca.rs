use anyhow::{Context, Result, anyhow};
use p12_keystore::{KeyStoreEntry, Pkcs12ImportPolicy};
use reqwest::Certificate;
use std::fs;

const JKS_MAGIC: u32 = 0xFEED_FEED;

/// Loads one or more trusted CA certificates from `path`.
///
/// The file is content-sniffed rather than dispatched on extension:
/// - A `-----BEGIN` prefix means a PEM bundle (one or more certs).
/// - JKS magic bytes (`0xFEEDFEED`) mean a Java KeyStore.
/// - Anything else is treated as a PKCS12 (`.p12`/`.pfx`) keystore.
///
/// JKS and PKCS12 both require `password`.
pub fn load_custom_ca_certificates(path: &str, password: Option<&str>) -> Result<Vec<Certificate>> {
    let bytes =
        fs::read(path).with_context(|| format!("could not read custom CA file '{}'", path))?;

    let certs = if bytes.starts_with(b"-----BEGIN") {
        Certificate::from_pem_bundle(&bytes)
            .with_context(|| format!("could not parse custom CA file '{}' as PEM", path))?
    } else {
        let password = password.ok_or_else(|| {
            anyhow!(
                "'{}' looks like a keystore (JKS/PKCS12), not a PEM file - set --use-custom-ca-password or TOAD_CA_PASSWORD",
                path
            )
        })?;

        let is_jks = bytes.len() >= 4
            && u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) == JKS_MAGIC;

        if is_jks {
            load_jks_certs(&bytes, password, path)?
        } else {
            load_pkcs12_certs(&bytes, password, path)?
        }
    };

    if certs.is_empty() {
        return Err(anyhow!(
            "no certificates found in custom CA file '{}'",
            path
        ));
    }

    Ok(certs)
}

fn load_jks_certs(bytes: &[u8], password: &str, path: &str) -> Result<Vec<Certificate>> {
    let mut ks = jks::KeyStore::new();
    ks.load(bytes, password.as_bytes()).with_context(|| {
        format!(
            "could not open '{}' as a JKS keystore - check that the file isn't corrupt and the password is correct",
            path
        )
    })?;

    let mut certs = Vec::new();
    for alias in ks.aliases() {
        if ks.is_trusted_certificate_entry(&alias) {
            let entry = ks.get_trusted_certificate_entry(&alias).with_context(|| {
                format!("could not read certificate entry '{}' in '{}'", alias, path)
            })?;
            certs.push(Certificate::from_der(&entry.certificate.content)?);
        } else if ks.is_private_key_entry(&alias) {
            let chain = ks
                .get_private_key_entry_certificate_chain(&alias)
                .with_context(|| {
                    format!(
                        "could not read certificate chain for '{}' in '{}'",
                        alias, path
                    )
                })?;
            for cert in chain {
                certs.push(Certificate::from_der(&cert.content)?);
            }
        }
    }
    Ok(certs)
}

fn load_pkcs12_certs(bytes: &[u8], password: &str, path: &str) -> Result<Vec<Certificate>> {
    let ks = p12_keystore::KeyStore::from_pkcs12(bytes, password, Pkcs12ImportPolicy::Raw)
        .with_context(|| {
            format!(
                "could not open '{}' as a PKCS12 keystore - check that the file isn't corrupt and the password is correct",
                path
            )
        })?;

    let mut certs = Vec::new();
    for (_, entry) in ks.entries() {
        match entry {
            KeyStoreEntry::Certificate(cert) => {
                certs.push(Certificate::from_der(cert.as_der())?);
            }
            KeyStoreEntry::PrivateKeyChain(chain) => {
                for cert in chain.certs() {
                    certs.push(Certificate::from_der(cert.as_der())?);
                }
            }
            KeyStoreEntry::Secret(_) => {}
        }
    }
    Ok(certs)
}
