//! Passkeys (WebAuthn), as narrow as a phone needs: ES256 keys (P-256, what Android's and
//! Apple's passkeys use), no attestation trusted (a key is trusted because the desk confirmed its
//! pairing), user verification required (biometrics or the screen lock), the origin and the
//! challenge checked on each ceremony.

use anyhow::{bail, ensure, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use p256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// The authenticator's flags (`authData[32]`).
const UP: u8 = 0x01;
const UV: u8 = 0x04;
const AT: u8 = 0x40;
/// COSE: EC2 key type, ES256 algorithm, P-256 curve.
const COSE_EC2: i128 = 2;
const COSE_ES256: i128 = -7;
const COSE_P256: i128 = 1;

/// Who the ceremony is for: the relying party's id (the host phones open) and its origin.
#[derive(Debug, Clone)]
pub struct Party {
    pub id: String,
    pub origin: String,
}

impl Party {
    /// From the address phones open: `https://mac.tail1234.ts.net` (or `http://localhost:…`,
    /// which browsers treat as secure, to try it on the computer).
    pub fn of(url: &str) -> Result<Self> {
        let u = url::Url::parse(url).with_context(|| format!("not an address: {url}"))?;
        let host = u.host_str().context("an address without a host")?.to_string();
        ensure!(u.scheme() == "https" || host == "localhost", "a passkey needs HTTPS: {url}");
        Ok(Self { id: host, origin: u.origin().ascii_serialization() })
    }
}

/// A passkey's public key, as kept: its id and SEC1 point, both base64url.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, Deserialize)]
pub struct Credential {
    pub id: String,
    pub public_key: String,
    pub sign_count: u32,
}

/// What the page sends after `navigator.credentials.create`, its buffers in base64url.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Registration {
    pub client_data_json: String,
    pub attestation_object: String,
}

/// What the page sends after `navigator.credentials.get`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assertion {
    pub credential_id: String,
    pub client_data_json: String,
    pub authenticator_data: String,
    pub signature: String,
}

#[derive(Deserialize)]
struct ClientData {
    #[serde(rename = "type")]
    kind: String,
    challenge: String,
    origin: String,
    #[serde(default, rename = "crossOrigin")]
    cross_origin: bool,
}

fn client_data(json_b64: &str, kind: &str, challenge: &str, party: &Party) -> Result<Vec<u8>> {
    let raw = B64.decode(json_b64).context("clientDataJSON")?;
    let data: ClientData = serde_json::from_slice(&raw).context("clientDataJSON")?;
    ensure!(data.kind == kind, "not a {kind} ceremony");
    ensure!(data.challenge == challenge, "another challenge");
    ensure!(data.origin == party.origin, "another origin: {}", data.origin);
    ensure!(!data.cross_origin, "a cross-origin ceremony");
    Ok(raw)
}

/// The challenge a ceremony answers, read before it is checked: which one the server issued.
pub fn challenge_of(client_data_json: &str) -> Result<String> {
    let data: ClientData = serde_json::from_slice(&B64.decode(client_data_json).context("clientDataJSON")?).context("clientDataJSON")?;
    Ok(data.challenge)
}

/// `authData`'s fixed part checked: the relying party's id, presence and verification of the
/// user. Returns its flags and signature counter.
fn auth_data(data: &[u8], party: &Party) -> Result<(u8, u32)> {
    ensure!(data.len() >= 37, "authenticator data too short");
    ensure!(data[..32] == Sha256::digest(party.id.as_bytes())[..], "another relying party");
    let flags = data[32];
    ensure!(flags & UP != 0, "the user was not present");
    ensure!(flags & UV != 0, "the user was not verified (biometrics or the screen lock)");
    Ok((flags, u32::from_be_bytes(data[33..37].try_into()?)))
}

/// A new passkey (pairing): its credential, once the ceremony checks out.
pub fn register(reg: &Registration, challenge: &str, party: &Party) -> Result<Credential> {
    client_data(&reg.client_data_json, "webauthn.create", challenge, party)?;
    let object: ciborium::Value = ciborium::from_reader(&B64.decode(&reg.attestation_object).context("attestationObject")?[..]).context("attestationObject")?;
    let data = map_get(&object, "authData").and_then(|v| v.as_bytes()).context("attestationObject without authData")?;
    let (flags, sign_count) = auth_data(data, party)?;
    ensure!(flags & AT != 0, "no credential in the authenticator data");
    // aaguid (16), then the credential id's length (2), the id, its COSE key.
    let rest = data.get(37 + 16..).context("authenticator data too short")?;
    ensure!(rest.len() >= 2, "authenticator data too short");
    let len = u16::from_be_bytes([rest[0], rest[1]]) as usize;
    let id = rest.get(2..2 + len).context("credential id cut short")?;
    let key: ciborium::Value = ciborium::from_reader(&rest[2 + len..]).context("credential public key")?;
    let point = cose_p256(&key)?;
    Ok(Credential { id: B64.encode(id), public_key: B64.encode(point.to_sec1_point(false).as_bytes()), sign_count })
}

/// A passkey used (login): checks it against its kept credential; returns the new counter.
pub fn verify(assertion: &Assertion, credential: &Credential, challenge: &str, party: &Party) -> Result<u32> {
    ensure!(assertion.credential_id == credential.id, "another passkey");
    let client = client_data(&assertion.client_data_json, "webauthn.get", challenge, party)?;
    let data = B64.decode(&assertion.authenticator_data).context("authenticatorData")?;
    let (_, count) = auth_data(&data, party)?;
    // A counter that does not move up means a copied authenticator; synced passkeys keep it at 0.
    if count != 0 || credential.sign_count != 0 {
        ensure!(count > credential.sign_count, "the passkey's counter went back: a copy?");
    }
    let key = VerifyingKey::from_sec1_bytes(&B64.decode(&credential.public_key)?).context("kept public key")?;
    let sig = Signature::from_der(&B64.decode(&assertion.signature).context("signature")?).context("signature")?;
    let sig = sig.normalize_s();
    let mut signed = data;
    signed.extend_from_slice(&Sha256::digest(&client));
    key.verify(&signed, &sig).map_err(|_| anyhow::anyhow!("the signature does not check out"))?;
    Ok(count)
}

fn map_get<'a>(v: &'a ciborium::Value, key: &str) -> Option<&'a ciborium::Value> {
    v.as_map()?.iter().find(|(k, _)| k.as_text() == Some(key)).map(|(_, v)| v)
}

fn cose_p256(key: &ciborium::Value) -> Result<VerifyingKey> {
    let map = key.as_map().context("COSE key is not a map")?;
    let get = |label: i128| map.iter().find(|(k, _)| k.as_integer().map(i128::from) == Some(label)).map(|(_, v)| v);
    let int = |label| get(label).and_then(|v| v.as_integer()).map(i128::from);
    if int(1) != Some(COSE_EC2) || int(3) != Some(COSE_ES256) || int(-1) != Some(COSE_P256) {
        bail!("only ES256 (P-256) passkeys are accepted");
    }
    let x = get(-2).and_then(|v| v.as_bytes()).context("COSE key without x")?;
    let y = get(-3).and_then(|v| v.as_bytes()).context("COSE key without y")?;
    ensure!(x.len() == 32 && y.len() == 32, "COSE key of the wrong size");
    let mut sec1 = vec![4u8];
    sec1.extend_from_slice(x);
    sec1.extend_from_slice(y);
    VerifyingKey::from_sec1_bytes(&sec1).context("COSE key not on P-256")
}

/// A fresh random value (challenge, token), base64url.
pub fn random() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the system's random source");
    B64.encode(bytes)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use p256::ecdsa::{signature::Signer, SigningKey};

    /// A software authenticator: what a phone's passkey does, to test the ceremonies.
    pub struct Authenticator {
        pub key: SigningKey,
        pub id: Vec<u8>,
        pub count: u32,
        pub flags: u8,
    }

    impl Authenticator {
        pub fn new() -> Self {
            Self { key: SigningKey::from_bytes(&[7u8; 32].into()).unwrap(), id: b"credential-1".to_vec(), count: 0, flags: UP | UV }
        }

        fn client(kind: &str, challenge: &str, origin: &str) -> Vec<u8> {
            serde_json::to_vec(&serde_json::json!({ "type": kind, "challenge": challenge, "origin": origin, "crossOrigin": false })).unwrap()
        }

        fn auth(&self, rp: &str, extra: &[u8], flags: u8) -> Vec<u8> {
            let mut d = Sha256::digest(rp.as_bytes()).to_vec();
            d.push(flags);
            d.extend_from_slice(&self.count.to_be_bytes());
            d.extend_from_slice(extra);
            d
        }

        pub fn create(&self, challenge: &str, party: &Party) -> Registration {
            let point = self.key.verifying_key().to_sec1_point(false);
            let cose = ciborium::Value::Map(vec![
                (1.into(), 2.into()),
                (3.into(), (-7).into()),
                ((-1).into(), 1.into()),
                ((-2).into(), ciborium::Value::Bytes(point.x().unwrap().to_vec())),
                ((-3).into(), ciborium::Value::Bytes(point.y().unwrap().to_vec())),
            ]);
            let mut attested = vec![0u8; 16];
            attested.extend_from_slice(&(self.id.len() as u16).to_be_bytes());
            attested.extend_from_slice(&self.id);
            ciborium::into_writer(&cose, &mut attested).unwrap();
            let object = ciborium::Value::Map(vec![
                ("fmt".into(), "none".into()),
                ("attStmt".into(), ciborium::Value::Map(vec![])),
                ("authData".into(), ciborium::Value::Bytes(self.auth(&party.id, &attested, self.flags | AT))),
            ]);
            let mut bytes = Vec::new();
            ciborium::into_writer(&object, &mut bytes).unwrap();
            Registration { client_data_json: B64.encode(Self::client("webauthn.create", challenge, &party.origin)), attestation_object: B64.encode(bytes) }
        }

        pub fn get(&self, challenge: &str, party: &Party) -> Assertion {
            let client = Self::client("webauthn.get", challenge, &party.origin);
            let data = self.auth(&party.id, &[], self.flags);
            let mut signed = data.clone();
            signed.extend_from_slice(&Sha256::digest(&client));
            let sig: Signature = self.key.sign(&signed);
            Assertion { credential_id: B64.encode(&self.id), client_data_json: B64.encode(client), authenticator_data: B64.encode(data), signature: B64.encode(sig.to_der().as_bytes()) }
        }
    }

    fn party() -> Party {
        Party::of("https://mac.tail1234.ts.net").unwrap()
    }

    #[test]
    fn a_passkey_registers_then_logs_in() {
        let (p, a) = (party(), Authenticator::new());
        let cred = register(&a.create("c1", &p), "c1", &p).unwrap();
        assert_eq!(cred.id, B64.encode(b"credential-1"));
        assert_eq!(verify(&a.get("c2", &p), &cred, "c2", &p).unwrap(), 0);
    }

    #[test]
    fn the_ceremony_is_bound_to_its_challenge_origin_and_party() {
        let (p, a) = (party(), Authenticator::new());
        assert!(register(&a.create("c1", &p), "other", &p).is_err());
        let elsewhere = Party::of("https://evil.example").unwrap();
        assert!(register(&a.create("c1", &elsewhere), "c1", &p).is_err());
        let cred = register(&a.create("c1", &p), "c1", &p).unwrap();
        assert!(verify(&a.get("c2", &p), &cred, "c3", &p).is_err());
        assert!(verify(&a.get("c2", &elsewhere), &cred, "c2", &p).is_err());
    }

    #[test]
    fn biometrics_are_required() {
        let p = party();
        let mut a = Authenticator::new();
        a.flags = UP;
        assert!(register(&a.create("c1", &p), "c1", &p).is_err());
    }

    #[test]
    fn another_key_or_a_tampered_signature_is_refused() {
        let (p, a) = (party(), Authenticator::new());
        let cred = register(&a.create("c1", &p), "c1", &p).unwrap();
        let mut b = Authenticator::new();
        b.key = SigningKey::from_bytes(&[9u8; 32].into()).unwrap();
        assert!(verify(&b.get("c2", &p), &cred, "c2", &p).is_err());
        let mut asn = a.get("c2", &p);
        asn.authenticator_data = B64.encode(a.auth(&p.id, &[], UP | UV | 0x08));
        assert!(verify(&asn, &cred, "c2", &p).is_err());
    }

    #[test]
    fn a_counter_going_back_is_refused() {
        let (p, mut a) = (party(), Authenticator::new());
        a.count = 5;
        let mut cred = register(&a.create("c1", &p), "c1", &p).unwrap();
        a.count = 6;
        cred.sign_count = verify(&a.get("c2", &p), &cred, "c2", &p).unwrap();
        a.count = 6;
        assert!(verify(&a.get("c3", &p), &cred, "c3", &p).is_err());
    }

    #[test]
    fn plain_http_is_refused_except_on_localhost() {
        assert!(Party::of("http://mac.tail1234.ts.net").is_err());
        assert_eq!(Party::of("http://localhost:47800").unwrap().origin, "http://localhost:47800");
    }
}
