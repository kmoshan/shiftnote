//! Password-protected database images. SQLite only receives plaintext in memory.
//!
//! Format v1: magic(12), Argon2id salt(16), GCM nonce(12), image length LE(8),
//! ciphertext and authentication tag(16). The entire header is authenticated.
//! KDF parameters are fixed by the format version, never supplied by a file.

use std::fs::{self, File, OpenOptions};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use fs2::FileExt;
use rusqlite::{Connection, MAIN_DB};
use serde::Serialize;
use zeroize::Zeroizing;

use crate::error::{AppError, AppResult};

const MAGIC: &[u8; 12] = b"SHIFTNOTE\x01\0\0";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const HEADER_LEN: usize = MAGIC.len() + SALT_LEN + NONCE_LEN + 8;
const TAG_LEN: usize = 16;
pub const MAX_IMAGE_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_FILE_BYTES: u64 = (MAX_IMAGE_BYTES + HEADER_LEN + TAG_LEN) as u64;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    pub configured: bool,
    pub unlocked: bool,
    pub legacy: bool,
}

// Intentionally no Debug, Serialize or Clone implementation for key material.
pub struct VaultSession {
    salt: [u8; SALT_LEN],
    key: Zeroizing<[u8; 32]>,
}

fn random<const N: usize>() -> AppResult<[u8; N]> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes)
        .map_err(|_| AppError::Other("无法取得系统安全随机数，请重试".into()))?;
    Ok(bytes)
}

pub fn validate_new_password(password: &str, confirm: Option<&str>) -> AppResult<()> {
    if password.chars().count() < 12 {
        return Err(AppError::validation("请使用至少 12 个字符的口令"));
    }
    if password.len() > 4096 {
        return Err(AppError::validation(
            "口令过长，请使用不超过 4096 字节的口令",
        ));
    }
    if confirm != Some(password) {
        return Err(AppError::validation("两次输入的口令不一致"));
    }
    Ok(())
}

fn authentication_error() -> AppError {
    AppError::validation("口令不正确或加密文件已损坏，原文件未被覆盖")
}

impl VaultSession {
    pub fn new(password: &str) -> AppResult<Self> {
        Self::derive(password, random()?)
    }

    fn derive(password: &str, salt: [u8; SALT_LEN]) -> AppResult<Self> {
        if password.is_empty() || password.len() > 4096 {
            return Err(AppError::validation("请输入有效口令（最多 4096 字节）"));
        }
        let mut key = Zeroizing::new([0; 32]);
        // Argon2id v1.3, 64 MiB, 3 iterations, one lane, 256-bit output.
        let params = Params::new(64 * 1024, 3, 1, Some(32))
            .map_err(|_| AppError::Other("口令派生参数无效".into()))?;
        let mut memory = Zeroizing::new(vec![argon2::Block::default(); params.block_count()]);
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into_with_memory(
                password.as_bytes(),
                &salt,
                key.as_mut(),
                memory.as_mut_slice(),
            )
            .map_err(|_| AppError::Other("无法派生加密密钥".into()))?;
        Ok(Self { salt, key })
    }

    pub fn open(path: &Path, password: &str) -> AppResult<(Self, Connection)> {
        let bytes = read_encrypted(path)?;
        let (salt, _, _) = parse_header(&bytes)?;
        let session = Self::derive(password, salt)?;
        let image = session.decrypt(&bytes)?;
        Ok((session, connection_from_image(&image)?))
    }

    fn encrypt(&self, image: &[u8]) -> AppResult<Vec<u8>> {
        if image.len() > MAX_IMAGE_BYTES {
            return Err(AppError::validation(
                "数据库超过 128 MiB 加密存储上限，请先整理数据",
            ));
        }
        let nonce = random::<NONCE_LEN>()?;
        let mut header = Vec::with_capacity(HEADER_LEN);
        header.extend_from_slice(MAGIC);
        header.extend_from_slice(&self.salt);
        header.extend_from_slice(&nonce);
        header.extend_from_slice(&(image.len() as u64).to_le_bytes());
        let cipher = Aes256Gcm::new_from_slice(self.key.as_ref())
            .map_err(|_| AppError::Other("无法初始化加密".into()))?;
        let encrypted = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: image,
                    aad: &header,
                },
            )
            .map_err(|_| AppError::Other("数据库加密失败".into()))?;
        header.extend_from_slice(&encrypted);
        Ok(header)
    }

    fn decrypt(&self, bytes: &[u8]) -> AppResult<Zeroizing<Vec<u8>>> {
        let (salt, nonce, _) = parse_header(bytes)?;
        if salt != self.salt {
            return Err(authentication_error());
        }
        let cipher = Aes256Gcm::new_from_slice(self.key.as_ref())
            .map_err(|_| AppError::Other("无法初始化解密".into()))?;
        let plaintext = cipher
            .decrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &bytes[HEADER_LEN..],
                    aad: &bytes[..HEADER_LEN],
                },
            )
            .map_err(|_| authentication_error())?;
        Ok(Zeroizing::new(plaintext))
    }

    pub fn read_backup(&self, path: &Path, password: Option<&str>) -> AppResult<Connection> {
        let bytes = read_encrypted(path)?;
        // A backup from this vault requires no second password. A different vault
        // has an independent salt, even when its owner chose the same password.
        let image = if let Some(password) = password.filter(|value| !value.is_empty()) {
            let (salt, _, _) = parse_header(&bytes)?;
            Self::derive(password, salt)?.decrypt(&bytes)?
        } else {
            self.decrypt(&bytes)?
        };
        connection_from_image(&image)
    }

    pub fn load_current(&self, path: &Path) -> AppResult<Connection> {
        let image = self.decrypt(&read_encrypted(path)?)?;
        connection_from_image(&image)
    }

    pub fn write(&self, conn: &mut Connection, path: &Path, overwrite: bool) -> AppResult<u64> {
        let image = conn.serialize(MAIN_DB)?;
        let encrypted = self.encrypt(&image)?;
        atomic_write(path, &encrypted, overwrite)?;
        Ok(encrypted.len() as u64)
    }
}

fn parse_header(bytes: &[u8]) -> AppResult<([u8; SALT_LEN], [u8; NONCE_LEN], usize)> {
    if bytes.len() < HEADER_LEN + TAG_LEN || bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(AppError::validation(
            "加密文件大小无效，最大支持 128 MiB 数据库",
        ));
    }
    if &bytes[..MAGIC.len()] != MAGIC {
        return Err(AppError::validation(
            "不是支持的 ShiftNote 加密文件或格式版本",
        ));
    }
    let mut salt = [0; SALT_LEN];
    salt.copy_from_slice(&bytes[MAGIC.len()..MAGIC.len() + SALT_LEN]);
    let mut nonce = [0; NONCE_LEN];
    nonce.copy_from_slice(&bytes[MAGIC.len() + SALT_LEN..HEADER_LEN - 8]);
    let size = u64::from_le_bytes(bytes[HEADER_LEN - 8..HEADER_LEN].try_into().unwrap());
    if size > MAX_IMAGE_BYTES as u64 || size + (HEADER_LEN + TAG_LEN) as u64 != bytes.len() as u64 {
        return Err(AppError::validation("加密文件长度不完整或已被修改"));
    }
    Ok((salt, nonce, size as usize))
}

fn read_encrypted(path: &Path) -> AppResult<Vec<u8>> {
    let mut file = File::open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > MAX_FILE_BYTES {
        return Err(AppError::validation(
            "加密文件大小无效，最大支持 128 MiB 数据库",
        ));
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    parse_header(&bytes)?;
    Ok(bytes)
}

pub fn is_encrypted(path: &Path) -> AppResult<bool> {
    let mut file = File::open(path)?;
    let mut magic = [0; MAGIC.len()];
    let read = file.read(&mut magic)?;
    Ok(read == MAGIC.len() && &magic == MAGIC)
}

pub fn connection_from_image(image: &[u8]) -> AppResult<Connection> {
    if image.len() < 100
        || image.len() > MAX_IMAGE_BYTES
        || !image.starts_with(b"SQLite format 3\0")
    {
        return Err(AppError::validation("解密后的数据库格式或大小无效"));
    }
    // deserialize does not support WAL images. New vaults use memory journals;
    // legacy databases must be copied through SQLite Backup before encryption.
    if image[18] != 1 || image[19] != 1 {
        return Err(AppError::validation("备份的数据库日志格式不受支持"));
    }
    let mut conn = Connection::open_in_memory()?;
    conn.deserialize_read_exact(MAIN_DB, Cursor::new(image), image.len(), false)?;
    Ok(conn)
}

pub fn lock_vault(path: &Path) -> AppResult<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.with_extension("vault.lock"))?;
    FileExt::try_lock_exclusive(&file).map_err(|_| {
        AppError::Conflict("此数据目录已被另一个 ShiftNote 实例使用，请先关闭它".into())
    })?;
    Ok(file)
}

struct TemporaryFile(PathBuf);
impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8], overwrite: bool) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::validation("数据文件路径无效"))?;
    fs::create_dir_all(parent)?;
    let temporary =
        TemporaryFile(parent.join(format!(".shiftnote-{}.encrypted-tmp", uuid::Uuid::new_v4())));
    {
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary.0)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    crate::export::publish(&temporary.0, path, overwrite)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_roundtrip_random_nonce_and_tamper_detection() {
        let session = VaultSession::new("正确口令-for-tests-1234").unwrap();
        let content = "中文事项和处理记录".as_bytes();
        let first = session.encrypt(content).unwrap();
        let second = session.encrypt(content).unwrap();
        assert_ne!(first, second, "每次保存必须有新的随机 nonce");
        assert_eq!(session.decrypt(&first).unwrap().as_slice(), content);
        assert!(!first.windows(content.len()).any(|part| part == content));
        let mut changed = first;
        *changed.last_mut().unwrap() ^= 1;
        assert!(session.decrypt(&changed).is_err());
        let mut header = second;
        header[MAGIC.len()] ^= 1;
        assert!(session.decrypt(&header).is_err());
    }

    #[test]
    fn password_validation_preserves_spaces_and_unicode() {
        let password = "  中文口令十二个字测试  ";
        validate_new_password(password, Some(password)).unwrap();
        assert!(validate_new_password(password, Some(password.trim())).is_err());
        assert!(validate_new_password("short", Some("short")).is_err());
        assert!(validate_new_password(&"a".repeat(4097), Some(&"a".repeat(4097))).is_err());
    }

    #[test]
    fn malformed_envelope_rejected_before_password_derivation() {
        assert!(parse_header(b"not a vault").is_err());
        let mut bytes = vec![0; HEADER_LEN + TAG_LEN];
        bytes[..MAGIC.len()].copy_from_slice(MAGIC);
        bytes[HEADER_LEN - 8..HEADER_LEN].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(parse_header(&bytes).is_err());
    }
}
