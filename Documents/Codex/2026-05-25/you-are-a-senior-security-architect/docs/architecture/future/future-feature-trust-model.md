# ESPASS Future-Feature Trust Model

This document describes the trust-boundary and cryptographic implications of
planned future features. None of these are implemented. This analysis must be
revisited before implementation begins.

## Passkeys (WebAuthn)

**Trust boundary impact:** Adds a new credential type stored in the vault. The
vault encryption model is unchanged. The passkey private key blob is stored
encrypted under the VaultKey like any other credential.

**Crypto implication:** The WebAuthn authenticator (platform or roaming) signs
assertions internally. ESPASS stores the `credentialId` and `userHandle`
encrypted; private keys never leave the authenticator.

**Threat model impact:** A compromised vault would expose the encrypted
credential ID, not the private key. The authenticator's secure element remains
the trust anchor.

## Mobile Apps

**Trust boundary impact:** Requires a mobile vault runtime with the same
security properties as the desktop. The mobile OS keychain should be used to
store the encrypted VaultKey, not the plaintext.

**Crypto implication:** The mobile KDF parameters must match or exceed desktop
defaults. Memory locking (`mlock`) is not available on iOS/Android userspace;
the OS process isolation model is relied upon instead.

**Threat model impact:** Jailbroken/rooted devices are an out-of-scope threat.
The mobile app must refuse to run or warn prominently on rooted devices.

## Enterprise RBAC

**Trust boundary impact:** Introduces a server-side policy layer. Vault access
decisions must remain client-side; the server enforces sharing policy by
controlling which encrypted VaultKey slots are served to which devices.

**Crypto implication:** Shared vaults require per-recipient key slots. Each
device gets the VaultKey encrypted under its `DeviceKey`. The server controls
who can receive which key slots but never holds the VaultKey in plaintext.

**Threat model impact:** A compromised admin cannot decrypt vaults. A
compromised server can deny access but cannot read plaintext. This is the
zero-knowledge constraint for enterprise sharing.

## HSM Integration

**Trust boundary impact:** The MasterKey derivation step (Argon2id) is replaced
or augmented by an HSM-bound key. The HSM becomes a hardware trust anchor.

**Crypto implication:** The HSM stores a hardware-bound root key. The VaultKey
is wrapped by `HMAC(hsm_root_key, argon2_output)`, so both the password and
the HSM key must be present to open the vault.

**Threat model impact:** Eliminates the key-in-memory-only risk at the cost of
requiring HSM availability. Offline unlock becomes impossible without pre-cached
key material.

## SSO

**Trust boundary impact:** Adds an identity provider as a trust anchor. The
identity token must be used to authenticate the device registration, not to
derive the VaultKey. The VaultKey must remain password-derived or
HSM-derived — SSO tokens must not be used as key material.

**Crypto implication:** SSO login gates access to the backend and device
registration flow. The VaultKey derivation is unchanged.

**Threat model impact:** A compromised IdP can prevent access (denial of
service) but cannot decrypt vaults. This preserves zero-knowledge.
