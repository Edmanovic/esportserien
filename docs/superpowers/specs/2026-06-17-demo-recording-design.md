# Demo Optagelse — Design Spec
**Dato:** 2026-06-17
**Projekt:** esportserien.dk
**Status:** Godkendt

---

## Overblik

Automatisk demo-optagelse på alle maps i en CS2-kamp. MatchZy optager `.dem` filen efter hvert map og POSTer den til backend'en, som uploader til Cloudflare R2. Den offentlige download-URL gemmes i `MatchMap.demoUrl` og vises som en download-knap på kampdetail-siden.

---

## Krav

- **Alle maps** — BO1, BO2, BO3: hvert map får sin egen demo
- **Automatisk** — ingen manuel handling fra admin eller spillere
- **Persistent storage** — Cloudflare R2 (offentlig bucket), overlever server-rydninger
- **Download-link** — vises på `/kampe/[id]` pr. map-kort når demo er klar
- **Ingen expiry** — offentlig URL, ingen signed URLs

---

## Cloudflare R2

| Variabel | Værdi |
|----------|-------|
| `R2_ACCOUNT_ID` | `e7654303c0c61e728719fb9bded864ee` |
| `R2_ACCESS_KEY_ID` | `04a87cb588ec55e4f26890fca0720e35` |
| `R2_SECRET_ACCESS_KEY` | `cbfeded936920fdfcf79a58eba3c700acee870669e57f5c304160d3ab8ac042f` |
| `R2_BUCKET_NAME` | `esportserien-demos` |
| `R2_PUBLIC_URL` | `https://pub-bd24722acbef4ef8a58949b0e79df5de.r2.dev` |

Filnavn i bucket: `demos/{matchId}/{mapNumber}-{mapName}.dem`

Public download URL: `https://pub-bd24722acbef4ef8a58949b0e79df5de.r2.dev/demos/{matchId}/{mapNumber}-{mapName}.dem`

---

## Datamodel

### Schema-ændring (migration)

```prisma
model MatchMap {
  id          String   @id @default(uuid())
  matchId     String
  mapNumber   Int
  mapName     String
  team1Rounds Int
  team2Rounds Int
  demoUrl     String?  // R2 public URL til .dem filen — null indtil upload er færdig

  match       Match    @relation(fields: [matchId], references: [id])
  @@unique([matchId, mapNumber])
}
```

---

## Backend

### Ny fil: `src/r2/r2.service.ts`

Wrapper om `@aws-sdk/client-s3` med R2-credentials fra env. Eksponerer én metode:

```ts
uploadDemo(key: string, buffer: Buffer, contentType: string): Promise<string>
// Returnerer public URL
```

Endpoint for S3-klient: `https://{R2_ACCOUNT_ID}.r2.cloudflarestorage.com`

### Nyt endpoint: `POST /matches/demo-upload`

Modtager multipart/form-data fra MatchZy:
- `demo_name` — filnavn, format: `{matchzyId}_map{n}_{mapname}.dem`
- `demo_file` — binær .dem fil

Flow:
1. Parser `matchzyId` og `mapNumber` fra `demo_name` via regex: `/^(\d+)_map(\d+)/`
2. Finder `Match` via `matchzyId`
3. Finder `MatchMap` via `matchId + mapNumber`
4. Uploader fil til R2 med key `demos/{matchId}/{mapNumber}-{mapName}.dem`
5. Gemmer public URL i `MatchMap.demoUrl`
6. Emitter WebSocket-event `demo_ready` med `{ matchId, mapNumber, demoUrl }`

Bruger `@nestjs/platform-express` + `multer` til multipart parsing (allerede en NestJS-dependency).

### Ændring: `buildMatchzyConfig()`

Tilføjer ét felt til MatchZy-config payloaden:

```ts
demo_upload_url: `${process.env.BACKEND_URL}/matches/demo-upload`,
```

---

## Frontend (`/kampe/[id]`)

### WebSocket
Lytter allerede på match-events. Tilføjer handling for `demo_ready`:
```ts
case "demo_ready":
  setMatch(prev => ({
    ...prev,
    maps: prev.maps.map(m =>
      m.mapNumber === data.mapNumber ? { ...m, demoUrl: data.demoUrl } : m
    )
  }));
```

### UI
På hvert map-kort: hvis `map.demoUrl` er sat, vises en download-knap:
```tsx
{map.demoUrl && (
  <a href={map.demoUrl} download className="...">
    Download demo
  </a>
)}
```

---

## Deploy

1. SSH til server
2. Tilføj R2 env-variabler til `/root/esports-platform-main/api/.env`
3. Installer `@aws-sdk/client-s3` på backend
4. Kør Prisma migration
5. Byg og restart backend: `docker compose build api && docker compose up -d api`
6. Byg og restart frontend: `docker compose build web && docker compose up -d web`
