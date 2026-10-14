//! AWS SDK adapter; instance-profile credentials only, no ambient key fallback.
use crate::{Entry, Kind, Provider, Secret, select};
use aws_credential_types::provider::ProvideCredentials;
use aws_sdk_secretsmanager::error::ProvideErrorMetadata;
use base64::Engine;
use zeroize::Zeroizing;

pub struct Aws {
    rt: tokio::runtime::Runtime,
    config: aws_config::SdkConfig,
}
impl Aws {
    pub fn new() -> Result<Self, &'static str> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| "Runtime")?;
        let config = rt.block_on(async {
            let provider = aws_config::imds::credentials::ImdsCredentialsProvider::builder().build();
            // Credentials are requested here so missing role/IMDS is exit 2 before output.
            provider
                .provide_credentials()
                .await
                .map_err(|_| "InstanceRoleUnavailable")?;
            Ok::<_, &'static str>(
                aws_config::defaults(aws_config::BehaviorVersion::latest())
                    .credentials_provider(provider)
                    .load()
                    .await,
            )
        })?;
        Ok(Self { rt, config })
    }
}
fn error(code: Option<&str>) -> &'static str {
    match code {
        Some("AccessDeniedException") => "AccessDeniedException",
        Some("ResourceNotFoundException") => "ResourceNotFoundException",
        Some("DecryptionFailure") => "DecryptionFailure",
        Some("InvalidCiphertextException") => "InvalidCiphertextException",
        _ => "ServiceError",
    }
}
impl Provider for Aws {
    fn get(&mut self, e: &Entry) -> Result<Secret, &'static str> {
        self.rt.block_on(async {
            let region = e.secret_id.split(':').nth(3).ok_or("InvalidSecretArn")?;
            let cfg = aws_sdk_secretsmanager::config::Builder::from(&self.config)
                .region(aws_sdk_secretsmanager::config::Region::new(region.to_owned()))
                .build();
            let mut r = aws_sdk_secretsmanager::Client::from_conf(cfg)
                .get_secret_value()
                .secret_id(&e.secret_id)
                .send()
                .await
                .map_err(|err| error(err.as_service_error().and_then(|e| e.code())))?;
            let version = r.version_id.take().ok_or("MissingVersion")?;
            let binary = r.secret_binary.take();
            let bytes = match (r.secret_string.take(), binary) {
                (Some(s), None) => Zeroizing::new(s.into_bytes()),
                (None, Some(b)) => Zeroizing::new(b.into_inner()),
                _ => return Err("MissingSecret"),
            };
            let bytes = select(bytes, e.json_key.as_deref())?;
            let bytes = if e.kind == Kind::KmsCiphertext {
                // The manifest's ciphertext is base64 text, whether stored directly or in JSON.
                let ciphertext = Zeroizing::new(
                    base64::engine::general_purpose::STANDARD
                        .decode(bytes.as_slice())
                        .map_err(|_| "CiphertextEncoding")?,
                );
                let cfg = aws_sdk_kms::config::Builder::from(&self.config)
                    .region(aws_sdk_kms::config::Region::new(region.to_owned()))
                    .build();
                let mut decrypted = aws_sdk_kms::Client::from_conf(cfg)
                    .decrypt()
                    .ciphertext_blob(aws_sdk_kms::primitives::Blob::new(ciphertext.to_vec()))
                    .send()
                    .await
                    .map_err(|err| error(err.as_service_error().and_then(|e| e.code())))?;
                Zeroizing::new(decrypted.plaintext.take().ok_or("MissingPlaintext")?.into_inner())
            } else {
                bytes
            };
            Ok(Secret { bytes, version })
        })
    }
}

impl std::fmt::Debug for Aws {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Aws { [redacted] }")
    }
}
