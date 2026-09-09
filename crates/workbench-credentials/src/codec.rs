use crate::{CredentialKind, Result, Source, StoredCredential, VaultError, MAX_CREDENTIAL_BYTES};
use serde::Serialize;
use zeroize::Zeroizing;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvelopeRef<'a> {
    version: u8,
    source: Source,
    account_name: &'a str,
    kind: CredentialKind,
    secret: &'a str,
}

pub(crate) fn encode(source: Source, credential: &StoredCredential) -> Result<Zeroizing<Vec<u8>>> {
    credential.validate_for(source)?;
    // Refuse large strings before allocating a JSON buffer; escaping is checked below.
    if credential.account_name().len() > MAX_CREDENTIAL_BYTES
        || credential.secret().len() > MAX_CREDENTIAL_BYTES
    {
        return Err(VaultError::TOO_LARGE);
    }
    let envelope = EnvelopeRef {
        version: 1,
        source,
        account_name: credential.account_name(),
        kind: credential.kind(),
        secret: credential.secret(),
    };
    let bytes =
        Zeroizing::new(serde_json::to_vec(&envelope).map_err(|_| VaultError::INVALID_CREDENTIAL)?);
    if bytes.len() > MAX_CREDENTIAL_BYTES {
        return Err(VaultError::TOO_LARGE);
    }
    Ok(bytes)
}

#[cfg(any(windows, test, feature = "test-support"))]
pub(crate) fn decode(source: Source, bytes: &[u8]) -> Result<StoredCredential> {
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct VersionProbe {
        version: u64,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Envelope {
        version: u64,
        source: Source,
        account_name: Zeroizing<String>,
        kind: CredentialKind,
        secret: Zeroizing<String>,
    }

    if bytes.len() > MAX_CREDENTIAL_BYTES {
        return Err(VaultError::TOO_LARGE);
    }
    let probe: VersionProbe = serde_json::from_slice(bytes).map_err(|_| VaultError::CORRUPT)?;
    if probe.version > 1 {
        return Err(VaultError::UNSUPPORTED_SCHEMA);
    }
    let envelope: Envelope = serde_json::from_slice(bytes).map_err(|_| VaultError::CORRUPT)?;
    if envelope.version != 1 || envelope.source != source {
        return Err(VaultError::CORRUPT);
    }
    let credential = StoredCredential {
        account_name: envelope.account_name,
        kind: envelope.kind,
        secret: envelope.secret,
    };
    credential
        .validate_for(source)
        .map_err(|_| VaultError::CORRUPT)?;
    Ok(credential)
}
