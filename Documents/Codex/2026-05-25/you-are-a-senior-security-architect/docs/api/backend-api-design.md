# Backend API Design

## Principles

- The API accepts encrypted vault payloads only.
- All inputs are validated with strict schemas.
- Authorization is enforced by tenant, team, role, device, and policy.
- Audit logs must never contain plaintext secrets.

## MVP Endpoints

| Method | Path | Purpose |
| --- | --- | --- |
| `POST` | `/v1/auth/login/start` | Begin passwordless or password-authenticated login flow |
| `POST` | `/v1/auth/login/finish` | Complete login and create session |
| `POST` | `/v1/devices` | Register device public keys and metadata |
| `GET` | `/v1/vaults` | List vault metadata visible to user |
| `POST` | `/v1/vaults` | Create vault metadata and encrypted header |
| `GET` | `/v1/vaults/{vault_id}/items` | List encrypted item records and revisions |
| `PUT` | `/v1/vaults/{vault_id}/items/{item_id}` | Upsert encrypted item blob |
| `GET` | `/v1/sync/stream` | WebSocket encrypted sync stream |
| `GET` | `/v1/audit/events` | Admin audit query with redaction |

## Database Sketch

```sql
create table users (
  id uuid primary key,
  email citext unique not null,
  kdf_salt bytea not null,
  kdf_params jsonb not null,
  created_at timestamptz not null default now()
);

create table vaults (
  id uuid primary key,
  tenant_id uuid not null,
  encrypted_header bytea not null,
  header_nonce bytea not null,
  version bigint not null default 1,
  created_at timestamptz not null default now()
);

create table vault_items (
  id uuid primary key,
  vault_id uuid not null references vaults(id),
  encrypted_blob bytea not null,
  blob_nonce bytea not null,
  aad jsonb not null,
  revision bigint not null,
  updated_at timestamptz not null default now()
);
```

