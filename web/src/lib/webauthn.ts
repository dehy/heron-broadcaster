// SPDX-License-Identifier: AGPL-3.0-or-later

// Passkeys: the API sends the options of `navigator.credentials` as JSON,
// binary fields in base64url, and takes the browser's answer back the same
// way (see docs/api.md#two-factor-authentication).

/** base64url (no padding) to bytes. */
export function fromBase64url(text: string): ArrayBuffer {
  const base64 = text.replaceAll("-", "+").replaceAll("_", "/");
  const padded = base64 + "=".repeat((4 - (base64.length % 4)) % 4);
  const binary = atob(padded);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.codePointAt(i) ?? 0;
  }
  return bytes.buffer;
}

/** Bytes to base64url, without padding. */
export function toBase64url(buffer: ArrayBuffer | ArrayBufferView): string {
  const bytes =
    buffer instanceof ArrayBuffer
      ? new Uint8Array(buffer)
      : new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength);
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCodePoint(byte);
  }
  return withoutPadding(btoa(binary).replaceAll("+", "-").replaceAll("/", "_"));
}

/** Without the `=` that pad base64 at its end. */
function withoutPadding(base64: string): string {
  let end = base64.length;
  while (end > 0 && base64[end - 1] === "=") end--;
  return base64.slice(0, end);
}

type Json = Record<string, unknown>;

interface Descriptor {
  id: string;
  type: string;
  transports?: string[];
}

function descriptors(list: unknown): PublicKeyCredentialDescriptor[] | undefined {
  if (!Array.isArray(list)) {
    return undefined;
  }
  return (list as Descriptor[]).map((entry) => ({
    ...entry,
    id: fromBase64url(entry.id),
    type: "public-key",
  })) as PublicKeyCredentialDescriptor[];
}

/** The API's options for `navigator.credentials.create()`. */
export function creationOptions(publicKey: Json): PublicKeyCredentialCreationOptions {
  const user = publicKey.user as Json;
  return {
    ...publicKey,
    challenge: fromBase64url(publicKey.challenge as string),
    user: { ...user, id: fromBase64url(user.id as string) },
    excludeCredentials: descriptors(publicKey.excludeCredentials),
  } as unknown as PublicKeyCredentialCreationOptions;
}

/** The API's options for `navigator.credentials.get()`. */
export function requestOptions(publicKey: Json): PublicKeyCredentialRequestOptions {
  return {
    ...publicKey,
    challenge: fromBase64url(publicKey.challenge as string),
    allowCredentials: descriptors(publicKey.allowCredentials),
  } as unknown as PublicKeyCredentialRequestOptions;
}

/** What the browser answered, as the API reads it. */
export function credentialJson(credential: PublicKeyCredential): Json {
  const response = credential.response as AuthenticatorResponse & {
    attestationObject?: ArrayBuffer;
    authenticatorData?: ArrayBuffer;
    signature?: ArrayBuffer;
    userHandle?: ArrayBuffer | null;
    getTransports?: () => string[];
  };
  const out: Json = { clientDataJSON: toBase64url(response.clientDataJSON) };
  if (response.attestationObject) {
    out.attestationObject = toBase64url(response.attestationObject);
    out.transports = response.getTransports?.() ?? [];
  }
  if (response.authenticatorData) {
    out.authenticatorData = toBase64url(response.authenticatorData);
  }
  if (response.signature) {
    out.signature = toBase64url(response.signature);
  }
  if (response.userHandle) {
    out.userHandle = toBase64url(response.userHandle);
  }
  return {
    id: credential.id,
    rawId: toBase64url(credential.rawId),
    type: credential.type,
    response: out,
    extensions: credential.getClientExtensionResults?.() ?? {},
  };
}

/** Whether this browser, on this address, can use passkeys. */
export function passkeysAvailable(): boolean {
  return typeof window !== "undefined" && window.isSecureContext && "PublicKeyCredential" in window;
}

/** Creates a passkey with the API's options; `null` if the user cancelled. */
export async function createPasskey(publicKey: Json): Promise<Json | null> {
  const credential = await navigator.credentials.create({ publicKey: creationOptions(publicKey) });
  return credential ? credentialJson(credential as PublicKeyCredential) : null;
}

/** Uses a passkey with the API's options; `null` if the user cancelled. */
export async function getPasskey(publicKey: Json): Promise<Json | null> {
  const credential = await navigator.credentials.get({ publicKey: requestOptions(publicKey) });
  return credential ? credentialJson(credential as PublicKeyCredential) : null;
}
