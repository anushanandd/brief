//! Document encryption. One random AES-256-GCM key, kept in the macOS Keychain, encrypts every
//! original with a fresh nonce; the document's id is bound in as associated data, so a file
//! can't be swapped for another. A backup can carry the key sealed under a passphrase, derived
//! with Argon2id, so its documents open on another Mac.
use aes_gcm::{
    aead::{Aead, Generate, Key, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use argon2::Argon2;
use sha2::{Digest, Sha256};

const KEYCHAIN_SERVICE: &str = "com.brief.finance.documents";
const KEYCHAIN_USER: &str = "documents-key";
const NONCE_BYTES: usize = 12;
const SALT_BYTES: usize = 16;

#[derive(Clone)]
pub struct DocumentKey(Key<Aes256Gcm>);

impl DocumentKey {
    pub fn generate() -> Self {
        Self(Key::<Aes256Gcm>::generate())
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let bytes: [u8; 32] = bytes.try_into().map_err(|_| "Invalid document key")?;
        Ok(Self(bytes.into()))
    }

    fn bytes(&self) -> &[u8] {
        self.0.as_slice()
    }

    /// Names the key without revealing it, so a backup can say whether this Mac holds it.
    pub fn fingerprint(&self) -> String {
        let digest = Sha256::digest([b"brief-documents:".as_slice(), self.bytes()].concat());
        digest[..8]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// `nonce || ciphertext`, bound to `id`.
    pub fn seal(&self, id: &str, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        let nonce = Nonce::generate();
        let sealed = Aes256Gcm::new(&self.0)
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad: id.as_bytes(),
                },
            )
            .map_err(|_| "Could not encrypt the document")?;
        Ok([nonce.as_slice(), &sealed].concat())
    }

    pub fn open(&self, id: &str, sealed: &[u8]) -> Result<Vec<u8>, String> {
        if sealed.len() <= NONCE_BYTES {
            return Err("This document's copy is damaged".into());
        }
        let (nonce, ciphertext) = sealed.split_at(NONCE_BYTES);
        let nonce: [u8; NONCE_BYTES] = nonce.try_into().map_err(|_| "Damaged document")?;
        Aes256Gcm::new(&self.0)
            .decrypt(
                &nonce.into(),
                Payload {
                    msg: ciphertext,
                    aad: id.as_bytes(),
                },
            )
            .map_err(|_| "This document can't be opened with this Mac's key".into())
    }

    /// The key sealed under `passphrase`: `salt || nonce || ciphertext`.
    pub fn wrap(&self, passphrase: &str) -> Result<Vec<u8>, String> {
        let salt = Key::<Aes256Gcm>::generate();
        let salt = &salt.as_slice()[..SALT_BYTES];
        let sealed =
            Self(passphrase_key(passphrase, salt)?).seal("brief-backup-key", self.bytes())?;
        Ok([salt, &sealed].concat())
    }

    pub fn unwrap(wrapped: &[u8], passphrase: &str) -> Result<Self, String> {
        if wrapped.len() <= SALT_BYTES {
            return Err("This backup's document key is damaged".into());
        }
        let (salt, sealed) = wrapped.split_at(SALT_BYTES);
        let bytes = Self(passphrase_key(passphrase, salt)?)
            .open("brief-backup-key", sealed)
            .map_err(|_| "That passphrase doesn't open this backup's documents")?;
        Self::from_bytes(&bytes)
    }
}

fn passphrase_key(passphrase: &str, salt: &[u8]) -> Result<Key<Aes256Gcm>, String> {
    let mut key = [0_u8; 32];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|_| "Could not derive a key from the passphrase")?;
    Ok(key.into())
}

/// The key this Mac keeps for documents, created the first time a document is saved.
pub fn keychain_key(create: bool) -> Result<Option<DocumentKey>, String> {
    let entry =
        keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_USER).map_err(|error| error.to_string())?;
    match entry.get_secret() {
        Ok(bytes) => DocumentKey::from_bytes(&bytes).map(Some),
        Err(keyring::Error::NoEntry) if create => {
            let key = DocumentKey::generate();
            entry
                .set_secret(key.bytes())
                .map_err(|error| format!("Could not save the document key in Keychain: {error}"))?;
            Ok(Some(key))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(format!(
            "Could not read the document key from Keychain: {error}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sealed_document_opens_only_with_its_key_and_id() {
        let key = DocumentKey::generate();
        let sealed = key.seal("a", b"W-2").unwrap();
        assert_eq!(key.open("a", &sealed).unwrap(), b"W-2");
        assert!(key.open("b", &sealed).is_err());
        assert!(DocumentKey::generate().open("a", &sealed).is_err());
        // A fresh nonce each time: the same bytes never seal alike.
        assert_ne!(key.seal("a", b"W-2").unwrap(), sealed);
    }

    #[test]
    fn a_wrapped_key_needs_its_passphrase() {
        let key = DocumentKey::generate();
        let wrapped = key.wrap("correct horse").unwrap();
        let unwrapped = DocumentKey::unwrap(&wrapped, "correct horse").unwrap();
        assert_eq!(unwrapped.fingerprint(), key.fingerprint());
        assert!(DocumentKey::unwrap(&wrapped, "wrong").is_err());
    }
}
