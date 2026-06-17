# Demo Recording Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Automatisk demo-optagelse via MatchZy — `.dem` filer uploades til Cloudflare R2 efter hvert map, URL gemmes i `MatchMap.demoUrl`, og vises som download-knap på `/kampe/[id]`.

**Architecture:** MatchZy poster demo-filen (multipart/form-data) til `POST /matches/demo-upload` på backend. Backend parser `matchzyId` og `mapNumber` fra filnavnet, uploader til R2 via `@aws-sdk/client-s3`, gemmer public URL i `MatchMap.demoUrl`, og emitter et `demo_ready` WebSocket-event. Frontendens kampside lytter på eventet og viser et download-link pr. map.

**Tech Stack:** NestJS + Prisma + PostgreSQL (backend), Next.js 14 App Router + Tailwind (frontend), `@aws-sdk/client-s3`, Cloudflare R2. SSH: `ssh -i ~/.ssh/id_ed25519 root@92.118.207.29`. Backend: `/root/esports-platform-main/api/`. Frontend: `/root/esport-web-main/`. Deploy: `cd /root/esport-prod && docker compose build <service> && docker compose up -d <service>`.

---

## Filer

| Handling | Fil |
|----------|-----|
| Modificér | `/root/esports-platform-main/api/prisma/schema.prisma` |
| Opret | `/root/esports-platform-main/api/src/matches/r2.service.ts` |
| Modificér | `/root/esports-platform-main/api/src/matches/matches.module.ts` |
| Modificér | `/root/esports-platform-main/api/src/matches/matches.service.ts` |
| Modificér | `/root/esports-platform-main/api/src/matches/matches.controller.ts` |
| Modificér | `/root/esports-platform-main/api/.env` |
| Modificér | `/root/esport-web-main/app/kampe/[id]/page.tsx` |

---

## Task 1: Schema migration

**Filer:**
- Modificér: `/root/esports-platform-main/api/prisma/schema.prisma`

- [ ] **Step 1: SSH til server**

```bash
ssh -i ~/.ssh/id_ed25519 root@92.118.207.29
```

- [ ] **Step 2: Tilføj `demoUrl` til `MatchMap` i schema.prisma**

Find `model MatchMap` i `/root/esports-platform-main/api/prisma/schema.prisma`. Tilføj `demoUrl` efter `team2Rounds`:

```prisma
model MatchMap {
  id          String   @id @default(uuid())
  matchId     String
  mapNumber   Int
  mapName     String
  team1Rounds Int
  team2Rounds Int
  demoUrl     String?

  match       Match    @relation(fields: [matchId], references: [id])

  @@unique([matchId, mapNumber])
}
```

- [ ] **Step 3: Kør migration**

```bash
cd /root/esports-platform-main/api
npx prisma migrate dev --name add_demo_url_to_match_map
```

Forventet output: `✔ Generated Prisma Client`

- [ ] **Step 4: Verificer at kolonnen eksisterer**

```bash
npx prisma db execute --stdin <<'SQL'
SELECT column_name FROM information_schema.columns
WHERE table_name = 'MatchMap' AND column_name = 'demoUrl';
SQL
```

Forventet: én række returneres med `demoUrl`.

- [ ] **Step 5: Commit**

```bash
cd /root/esports-platform-main/api
git add prisma/schema.prisma prisma/migrations/
git commit -m "feat: add demoUrl to MatchMap schema"
```

---

## Task 2: R2 env-variabler og service

**Filer:**
- Modificér: `/root/esports-platform-main/api/.env`
- Opret: `/root/esports-platform-main/api/src/matches/r2.service.ts`
- Modificér: `/root/esports-platform-main/api/src/matches/matches.module.ts`

- [ ] **Step 1: Tilføj R2 env-variabler til `.env`**

Åbn `/root/esports-platform-main/api/.env` og tilføj nederst:

```
R2_ACCOUNT_ID=e7654303c0c61e728719fb9bded864ee
R2_ACCESS_KEY_ID=04a87cb588ec55e4f26890fca0720e35
R2_SECRET_ACCESS_KEY=cbfeded936920fdfcf79a58eba3c700acee870669e57f5c304160d3ab8ac042f
R2_BUCKET_NAME=esportserien-demos
R2_PUBLIC_URL=https://pub-bd24722acbef4ef8a58949b0e79df5de.r2.dev
```

- [ ] **Step 2: Installer `@aws-sdk/client-s3` og `@types/multer`**

```bash
cd /root/esports-platform-main/api
npm install @aws-sdk/client-s3
npm install --save-dev @types/multer
```

Forventet: package installeres uden fejl.

- [ ] **Step 3: Opret `r2.service.ts`**

Opret filen `/root/esports-platform-main/api/src/matches/r2.service.ts` med dette indhold:

```typescript
import { Injectable } from '@nestjs/common';
import { S3Client, PutObjectCommand } from '@aws-sdk/client-s3';

@Injectable()
export class R2Service {
  private client: S3Client;
  private bucket = process.env.R2_BUCKET_NAME!;
  private publicUrl = process.env.R2_PUBLIC_URL!;

  constructor() {
    this.client = new S3Client({
      region: 'auto',
      endpoint: `https://${process.env.R2_ACCOUNT_ID}.r2.cloudflarestorage.com`,
      credentials: {
        accessKeyId: process.env.R2_ACCESS_KEY_ID!,
        secretAccessKey: process.env.R2_SECRET_ACCESS_KEY!,
      },
    });
  }

  async uploadDemo(key: string, buffer: Buffer, contentType: string): Promise<string> {
    await this.client.send(new PutObjectCommand({
      Bucket: this.bucket,
      Key: key,
      Body: buffer,
      ContentType: contentType,
    }));
    return `${this.publicUrl}/${key}`;
  }
}
```

- [ ] **Step 4: Registrer `R2Service` i `matches.module.ts`**

Erstat indholdet af `/root/esports-platform-main/api/src/matches/matches.module.ts` med:

```typescript
import { Module } from '@nestjs/common';
import { JwtModule } from '@nestjs/jwt';
import { MatchesController } from './matches.controller';
import { AdminServersController } from '../admin/admin-servers.controller';
import { AdminUsersController } from '../admin/admin-users.controller';
import { MatchesService } from './matches.service';
import { MatchesGateway } from './matches.gateway';
import { R2Service } from './r2.service';
import { PrismaModule } from '../prisma/prisma.module';

@Module({
  imports: [PrismaModule, JwtModule.register({ secret: process.env.JWT_SECRET })],
  controllers: [MatchesController, AdminServersController, AdminUsersController],
  providers: [MatchesService, MatchesGateway, R2Service],
  exports: [MatchesGateway],
})
export class MatchesModule {}
```

- [ ] **Step 5: Commit**

```bash
cd /root/esports-platform-main/api
git add src/matches/r2.service.ts src/matches/matches.module.ts .env
git commit -m "feat: add R2Service for Cloudflare demo storage"
```

---

## Task 3: Demo upload endpoint

**Filer:**
- Modificér: `/root/esports-platform-main/api/src/matches/matches.service.ts`
- Modificér: `/root/esports-platform-main/api/src/matches/matches.controller.ts`

- [ ] **Step 1: Tilføj `R2Service` injection og `handleDemoUpload()` til `matches.service.ts`**

Øverst i `matches.service.ts`, find constructor-linjen der injicerer PrismaService og MatchesGateway. Tilføj `R2Service`:

Find denne linje i constructor:
```typescript
constructor(private prisma: PrismaService, private gateway: MatchesGateway) {}
```

Erstat med:
```typescript
constructor(
  private prisma: PrismaService,
  private gateway: MatchesGateway,
  private r2: R2Service,
) {}
```

Tilføj også importen øverst i filen, efter eksisterende imports:
```typescript
import { R2Service } from './r2.service';
```

- [ ] **Step 2: Tilføj `handleDemoUpload()` metode til `matches.service.ts`**

Indsæt denne metode i `MatchesService`-klassen, f.eks. lige før `handleMatchzyWebhook`:

```typescript
async handleDemoUpload(file: Express.Multer.File, demoName: string) {
  const nameMatch = demoName?.match(/^(\d+)_map(\d+)/);
  if (!nameMatch || !file?.buffer) {
    console.warn('[demo-upload] Ugyldigt filnavn eller manglende fil:', demoName);
    return { success: false };
  }

  const matchzyId = Number(nameMatch[1]);
  const mapNumber = Number(nameMatch[2]);

  const rows = await this.prisma.$queryRaw<{ id: string; matchId: string; mapName: string }[]>`
    SELECT mm.id, mm."matchId", mm."mapName"
    FROM "MatchMap" mm
    JOIN "Match" m ON mm."matchId" = m.id
    WHERE m."matchzyId" = ${matchzyId} AND mm."mapNumber" = ${mapNumber}
    LIMIT 1
  `;

  if (!rows.length) {
    console.warn(`[demo-upload] Ingen MatchMap fundet for matchzyId=${matchzyId} mapNumber=${mapNumber}`);
    return { success: false };
  }

  const { id: mapId, matchId, mapName } = rows[0];
  const key = `demos/${matchId}/${mapNumber}-${mapName}.dem`;

  const demoUrl = await this.r2.uploadDemo(key, file.buffer, 'application/octet-stream');

  await this.prisma.matchMap.update({
    where: { id: mapId },
    data: { demoUrl },
  });

  this.gateway.emitMatchUpdate(matchId, { type: 'demo_ready', mapNumber, demoUrl });

  console.log(`[demo-upload] Demo uploadet: ${demoUrl}`);
  return { success: true, demoUrl };
}
```

- [ ] **Step 3: Tilføj upload endpoint i `matches.controller.ts`**

Tilføj disse imports øverst i `matches.controller.ts` (efter eksisterende imports fra `@nestjs/common`):

```typescript
import { UseInterceptors, UploadedFile } from '@nestjs/common';
import { FileInterceptor } from '@nestjs/platform-express';
```

Tilføj dette endpoint i `MatchesController`-klassen, f.eks. efter `matchzyWebhook`:

```typescript
@Post("demo-upload")
@UseInterceptors(FileInterceptor("demo_file", {
  limits: { fileSize: 500 * 1024 * 1024 },
}))
async demoUpload(
  @UploadedFile() file: Express.Multer.File,
  @Body("demo_name") demoName: string,
) {
  return this.matchesService.handleDemoUpload(file, demoName);
}
```

- [ ] **Step 4: Verificer at TypeScript kompilerer**

```bash
cd /root/esports-platform-main/api
npx tsc --noEmit
```

Forventet: ingen fejl.

- [ ] **Step 5: Commit**

```bash
cd /root/esports-platform-main/api
git add src/matches/matches.service.ts src/matches/matches.controller.ts
git commit -m "feat: add POST /matches/demo-upload endpoint with R2 upload"
```

---

## Task 4: Tilføj `demo_upload_url` til MatchZy config

**Filer:**
- Modificér: `/root/esports-platform-main/api/src/matches/matches.service.ts`

- [ ] **Step 1: Find `buildMatchzyConfig` return-objektet**

Find denne linje i `matches.service.ts`:
```typescript
remote_log_url: `${backendUrl}/matches/matchzy-webhook`,
```

- [ ] **Step 2: Tilføj `demo_upload_url` i return-objektet**

Tilføj én linje direkte efter `remote_log_url`:

```typescript
remote_log_url: `${backendUrl}/matches/matchzy-webhook`,
demo_upload_url: `${backendUrl}/matches/demo-upload`,
```

- [ ] **Step 3: Commit**

```bash
cd /root/esports-platform-main/api
git add src/matches/matches.service.ts
git commit -m "feat: add demo_upload_url to MatchZy config"
```

---

## Task 5: Frontend — demo download knap

**Filer:**
- Modificér: `/root/esport-web-main/app/kampe/[id]/page.tsx`

- [ ] **Step 1: Tilføj `demo_ready` WebSocket handler**

Find denne blok i `page.tsx` inde i `socket.on(`match:${id}`, ...)`:

```typescript
if (data.type === "match_done") setMatch((prev: any) => prev ? { ...prev, status: "COMPLETED" } : prev);
```

Tilføj disse linjer direkte efter:

```typescript
if (data.type === "demo_ready") {
  setMatch((prev: any) => {
    if (!prev) return prev;
    return {
      ...prev,
      maps: prev.maps.map((m: any) =>
        m.mapNumber === data.mapNumber ? { ...m, demoUrl: data.demoUrl } : m
      ),
    };
  });
}
```

- [ ] **Step 2: Tilføj download-knap på map-kort**

Find den del af map-kortet der viser score og hold. Præcist efter det afsluttende `</div>` for `flex items-center justify-between px-5 py-4`-blokken (score-rækken), tilføj demo-knappen:

Find denne linje:
```typescript
                  </div>
                </div>
              );
            })}
```

Der er én `</div>` der lukker `flex items-center justify-between px-5 py-4`. Tilføj demo-linket inden den ydre `</div>` der afslutter det hele map-kort:

```tsx
                  {result?.demoUrl && (
                    <div className="px-5 pb-4">
                      <a
                        href={result.demoUrl}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="flex items-center justify-center gap-2 w-full py-2 rounded-xl bg-[var(--bg-secondary)] text-xs font-semibold text-[#c23c84] hover:text-[#e05aa0] hover:bg-[var(--border)] transition"
                      >
                        ↓ Download demo
                      </a>
                    </div>
                  )}
```

Knappen placeres præcist inden den afsluttende `</div>` for `bg-[var(--bg-primary)] rounded-2xl overflow-hidden border`-kortet, dvs. strukturen bliver:

```tsx
<div key={map.id} className="bg-[var(--bg-primary)] rounded-2xl overflow-hidden border border-[var(--border)]">
  {/* map billede */}
  {/* score række */}
  <div className="flex items-center justify-between px-5 py-4">
    ...
  </div>
  {/* demo knap — KUN hvis demoUrl er sat */}
  {result?.demoUrl && (
    <div className="px-5 pb-4">
      <a
        href={result.demoUrl}
        target="_blank"
        rel="noopener noreferrer"
        className="flex items-center justify-center gap-2 w-full py-2 rounded-xl bg-[var(--bg-secondary)] text-xs font-semibold text-[#c23c84] hover:text-[#e05aa0] hover:bg-[var(--border)] transition"
      >
        ↓ Download demo
      </a>
    </div>
  )}
</div>
```

- [ ] **Step 3: Commit**

```bash
cd /root/esport-web-main
git add app/kampe/\[id\]/page.tsx
git commit -m "feat: show demo download button on match map cards"
```

---

## Task 6: Deploy og verificer

- [ ] **Step 1: Byg og restart API**

```bash
cd /root/esport-prod
docker compose build api && docker compose up -d api
```

Forventet: container starter uden fejl.

- [ ] **Step 2: Verificer API er oppe**

```bash
curl https://api.esportserien.dk/health
```

Forventet: `{"status":"ok"}` eller HTTP 200.

- [ ] **Step 3: Test demo-upload endpoint med en lille fil**

```bash
dd if=/dev/urandom of=/tmp/test.dem bs=1024 count=10
curl -X POST https://api.esportserien.dk/matches/demo-upload \
  -F "demo_name=123_map1_Inferno.dem" \
  -F "demo_file=@/tmp/test.dem"
```

Forventet output: `{"success":false}` (ingen match med matchzyId 123) — det bekræfter at endpointet svarer og ikke crasher.

- [ ] **Step 4: Byg og restart frontend**

```bash
cd /root/esport-prod
docker compose build web && docker compose up -d web
```

- [ ] **Step 5: Verificer frontend er oppe**

Åbn `https://esportserien.dk` i browser. Tjek at eksisterende kampside stadig loader korrekt og map-kort ser normale ud (ingen demo-knap på gamle kampe — det er korrekt).

- [ ] **Step 6: End-to-end verificering**

Næste gang en kamp spilles via MatchZy:
1. MatchZy henter config via `/matches/:id/matchzy-config` — tjek at `demo_upload_url` er med:
   ```bash
   curl "https://api.esportserien.dk/matches/<id>/matchzy-config?secret=<INTERNAL_API_SECRET>"
   ```
   Forventet: `"demo_upload_url":"https://api.esportserien.dk/matches/demo-upload"` er i svaret.
2. Efter map er færdigt: tjek backend logs for `[demo-upload] Demo uploadet:` linje.
3. Åbn kampens side — download-knappen skal vises på det map.
