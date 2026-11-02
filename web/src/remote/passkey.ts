/** The passkey ceremonies the remote asks for: pairing (create) and login (get). */
import { t } from '../i18n.svelte'

const b64 = (buf: ArrayBuffer): string => btoa(String.fromCharCode(...new Uint8Array(buf))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
const bytes = (s: string): Uint8Array<ArrayBuffer> => Uint8Array.from(atob(s.replace(/-/g, '+').replace(/_/g, '/')), (c) => c.charCodeAt(0))

/** A call of the remote's API: its JSON, or its error as thrown text. */
export async function api<T>(path: string, body?: unknown): Promise<T> {
  const res = await fetch(path, body === undefined ? {} : { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) })
  const data = await res.json().catch(() => ({}))
  if (!res.ok) throw new Error(data.error ?? `${res.status}`)
  return data as T
}

export const supported = () => typeof window.PublicKeyCredential === 'function'

/** Creates this phone's passkey for the pairing's challenge. */
export async function create(o: { challenge: string; rpId: string; user: string; name: string }) {
  const cred = (await navigator.credentials.create({
    publicKey: {
      rp: { id: o.rpId, name: 'Prism' },
      user: { id: bytes(o.user), name: o.name, displayName: o.name },
      challenge: bytes(o.challenge),
      pubKeyCredParams: [{ type: 'public-key', alg: -7 }],
      authenticatorSelection: { residentKey: 'preferred', userVerification: 'required' },
      attestation: 'none',
      timeout: 120_000,
    },
  })) as PublicKeyCredential | null
  if (!cred) throw new Error(t('no passkey created'))
  const r = cred.response as AuthenticatorAttestationResponse
  return { clientDataJson: b64(r.clientDataJSON), attestationObject: b64(r.attestationObject) }
}

/** Unlocks with the passkey: biometrics or the screen lock. */
export async function get(o: { challenge: string; rpId: string; allow: string[] }) {
  const cred = (await navigator.credentials.get({
    publicKey: {
      challenge: bytes(o.challenge),
      rpId: o.rpId,
      allowCredentials: o.allow.map((id) => ({ type: 'public-key' as const, id: bytes(id) })),
      userVerification: 'required',
      timeout: 60_000,
    },
  })) as PublicKeyCredential | null
  if (!cred) throw new Error(t('no passkey used'))
  const r = cred.response as AuthenticatorAssertionResponse
  return { credentialId: b64(cred.rawId), clientDataJson: b64(r.clientDataJSON), authenticatorData: b64(r.authenticatorData), signature: b64(r.signature) }
}
