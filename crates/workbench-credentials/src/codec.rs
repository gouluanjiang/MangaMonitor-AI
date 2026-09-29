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
    #[serde(skip_serializing_if = "Option::is_none")]
    login_password: Option<&'a str>,
}

pub(crate) fn encode(source: Source, credential: &StoredCredential) -> Result<Zeroizing<Vec<u8>>> {
    credential.validate_for(source)?;
    // Refuse large strings before allocating a JSON buffer; escaping is checked below.
    if credential.account_name().len() > MAX_CREDENTIAL_BYTES
        || credential.secret().len() > MAX_CREDENTIAL_BYTES
        || credential
            .login_password()
            .is_some_and(|password| password.len() > MAX_CREDENTIAL_BYTES)
    {
        return Err(VaultError::TOO_LARGE);
    }
    let envelope = EnvelopeRef {
        version: if credential.login_password().is_some() {
            2
        } else {
            1
        },
        source,
        account_name: credential.account_name(),
        kind: credential.kind(),
        secret: credential.secret(),
        login_password: credential.login_password(),
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
    struct EnvelopeV1 {
        version: u64,
        source: Source,
        account_name: Zeroizing<String>,
        kind: CredentialKind,
        secret: Zeroizing<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct EnvelopeV2 {
        version: u64,
        source: Source,
        account_name: Zeroizing<String>,
        kind: CredentialKind,
        secret: Zeroizing<String>,
        login_password: Zeroizing<String>,
    }

    if bytes.len() > MAX_CREDENTIAL_BYTES {
        return Err(VaultError::TOO_LARGE);
    }
    let probe: VersionProbe = serde_json::from_slice(bytes).map_err(|_| VaultError::CORRUPT)?;
    if probe.version > 2 {
        return Err(VaultError::UNSUPPORTED_SCHEMA);
    }
    let credential = match probe.version {
        1 => {
            let envelope: EnvelopeV1 =
                serde_json::from_slice(bytes).map_err(|_| VaultError::CORRUPT)?;
            if envelope.version != 1 || envelope.source != source {
                return Err(VaultError::CORRUPT);
            }
            StoredCredential {
                account_name: envelope.account_name,
                kind: envelope.kind,
                secret: envelope.secret,
                login_password: None,
            }
        }
        2 => {
            let envelope: EnvelopeV2 =
                serde_json::from_slice(bytes).map_err(|_| VaultError::CORRUPT)?;
            if envelope.version != 2 || envelope.source != source {
                return Err(VaultError::CORRUPT);
            }
            StoredCredential {
                account_name: envelope.account_name,
                kind: envelope.kind,
                secret: envelope.secret,
                login_password: Some(envelope.login_password),
            }
        }
        _ => return Err(VaultError::CORRUPT),
    };
    credential
        .validate_for(source)
        .map_err(|_| VaultError::CORRUPT)?;
    Ok(credential)
}
