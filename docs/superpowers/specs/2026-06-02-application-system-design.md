# Ansøgningssystem — Design Spec
**Dato:** 2026-06-02  
**Projekt:** esportserien.dk  
**Status:** Godkendt

---

## Overblik

Et dynamisk ansøgningssystem der giver hold-captains mulighed for at ansøge om deltagelse i en sæson. Admins bygger multi-step polls med spørgsmål, modtager ansøgninger og bekræfter dem via email.

---

## Brugerflows

### Captain-flow
1. Captain besøger `/hold/[slug]`
2. Ser knappen **"Ansøg til sæson"** ved siden af "Administrer hold" (vises kun hvis: captain + aktiv OPEN poll + holdet ikke allerede har ansøgt)
3. Navigerer til `/hold/[slug]/ansoeg`
4. Udfylder multi-step formular:
   - **Step 0 (altid):** Vælg ansøgertype: `Hold` | `Organisation` | `Efterskole/Institution`
   - **Step 1..N:** Dynamiske spørgsmål fra poll
   - **Sidste step:** Opsummering + "Indsend ansøgning"
5. Modtager bekræftelse på skærmen
6. Hvis admin bekræfter: captain modtager branded HTML-email

### Admin-flow
1. Admin besøger `/admin/ansoegninger`
2. **Tab "Polls":** Opretter/redigerer polls
   - Titel, beskrivelse, sæson-kobling
   - Tilføj/fjern/reorder steps
   - Pr. step: tilføj spørgsmål (type, label, options, required)
   - Skift status: DRAFT → OPEN → CLOSED
3. **Tab "Ansøgninger":** Vælg poll → se alle svar
   - Tabel: holdnavn, captain, type, dato, status
   - Klik på række → ekspanderet svar-visning
   - Bekræft / Afvis knapper

---

## Datamodel

### Eksisterende modeller — nødvendige back-references
```prisma
// Tilføj til Season:
applicationPolls ApplicationPoll[]

// Tilføj til Team:
applications ApplicationSubmission[]

// Tilføj til User:
applications ApplicationSubmission[]
```

### `ApplicationPoll`
```prisma
model ApplicationPoll {
  id          String   @id @default(uuid())
  title       String
  description String?
  seasonId    String?
  status      PollStatus @default(DRAFT)
  steps       Json     // Step[]
  createdAt   DateTime @default(now())
  updatedAt   DateTime @updatedAt

  season      Season?  @relation(fields: [seasonId], references: [id])
  submissions ApplicationSubmission[]
}

enum PollStatus {
  DRAFT
  OPEN
  CLOSED
}
```

### `ApplicationSubmission`
```prisma
model ApplicationSubmission {
  id            String          @id @default(uuid())
  pollId        String
  teamId        String
  captainId     String
  applicantType ApplicantType
  status        SubmissionStatus @default(PENDING)
  answers       Json            // { "q-id": "svar" | ["valg1"] }
  createdAt     DateTime        @default(now())
  updatedAt     DateTime        @updatedAt

  poll    ApplicationPoll @relation(fields: [pollId], references: [id])
  team    Team            @relation(fields: [teamId], references: [id])
  captain User            @relation(fields: [captainId], references: [id])

  @@unique([pollId, teamId])
}

enum ApplicantType {
  TEAM
  ORG
  SCHOOL
}

enum SubmissionStatus {
  PENDING
  CONFIRMED
  REJECTED
}
```

### Steps JSON-struktur (gemt i `ApplicationPoll.steps`)
```typescript
type QuestionType = "short_text" | "long_text" | "radio" | "checkbox";

interface Question {
  id: string;        // unik inden for poll, fx "q-1"
  type: QuestionType;
  label: string;
  required: boolean;
  options?: string[]; // kun for radio/checkbox
}

interface Step {
  id: string;        // fx "step-1"
  title: string;
  questions: Question[];
}
```

---

## Backend

### Ny mappe: `/api/src/applications/`
```
applications.module.ts
applications.controller.ts        ← captain endpoints
applications.service.ts
admin-applications.controller.ts  ← admin endpoints (ADMIN guard)
```

### Captain endpoints
| Method | Path | Beskrivelse |
|--------|------|-------------|
| GET | `/applications/active` | Hent senest oprettede OPEN poll (steps uden svar). Tom 404 hvis ingen OPEN poll. |
| GET | `/applications/my/:teamId` | Har holdet allerede ansøgt på aktiv poll? |
| POST | `/applications/:pollId/submit` | Indsend ansøgning |

**POST submit — validering:**
- JWT required, `userId === team.captainId`
- Poll status === OPEN
- Ingen eksisterende submission for `[pollId, teamId]`
- Required spørgsmål er besvaret

### Admin endpoints (ADMIN role guard)
| Method | Path | Beskrivelse |
|--------|------|-------------|
| GET | `/admin/applications` | List alle polls |
| POST | `/admin/applications` | Opret poll |
| PUT | `/admin/applications/:id` | Opdater poll (steps, status, titel) |
| DELETE | `/admin/applications/:id` | Slet poll (kun DRAFT) |
| GET | `/admin/applications/:id/submissions` | Alle svar på en poll |
| PATCH | `/admin/applications/:id/submissions/:sid` | Bekræft/afvis + send email |

### Email ved bekræftelse
Bruger eksisterende `MailerService` med samme branded HTML-skabelon som verificerings-emails.

Indhold:
> **Emne:** Jeres ansøgning til [Sæson navn] er bekræftet — Esportserien  
> **Body:** Tillykke, [holdnavn]! Jeres ansøgning som [type] er bekræftet. Vi kontakter jer snart med næste skridt.

---

## Frontend

### Ændrede filer
| Fil | Ændring |
|-----|---------|
| `/hold/[slug]/page.tsx` | Tilføj "Ansøg til sæson"-knap + status-badge for captains |

### Nye filer
| Fil | Formål |
|-----|--------|
| `/hold/[slug]/ansoeg/page.tsx` | Multi-step ansøgningsformular |
| `/admin/ansoegninger/page.tsx` | Poll-builder + ansøgningsoversigt |

### Hold-siden — ny knap
Vises kun for captain, kun hvis aktiv poll eksisterer:
```
[Administrer hold]  [Ansøg til sæson →]
```
Hvis holdet allerede har ansøgt vises i stedet:
```
[Administrer hold]  [Ansøgning indsendt ✓]  (klikbar → viser status)
```

### Ansøgningsformular (`/hold/[slug]/ansoeg`)
- Progress bar: "Trin X af Y"
- Step 0: Type-valg (tre kort med ikon)
- Step 1..N: Dynamiske spørgsmål
  - `short_text` → `<input type="text">`
  - `long_text` → `<textarea>`
  - `radio` → radioknapper
  - `checkbox` → checkboxes
- Validering af required-felter ved trin-skift (ikke submit)
- Tilbage/Frem navigation
- Sidste trin: opsummering af alle svar + indsend-knap
- Success-state med besked efter indsendelse

### Admin-siden (`/admin/ansoegninger`)
**Tab 1 — Polls:**
- Liste med poll-kort (titel, sæson, status-badge, antal svar)
- "Ny poll"-knap → formular åbner inline/modal:
  - Titel + beskrivelse
  - Sæson-dropdown
  - Status-toggle (DRAFT/OPEN/CLOSED)
  - Step-builder:
    - Tilføj step (titel)
    - Pr. step: tilføj spørgsmål (type dropdown, label, required toggle, options for radio/checkbox)
    - Slet steps og spørgsmål
    - Reorder via op/ned-pile (ingen drag-library)

**Tab 2 — Ansøgninger:**
- Poll-dropdown øverst
- Statistik-linje: X afventer, Y bekræftet, Z afvist
- Tabel: holdlogo, holdnavn, captain, type-badge, dato, status-badge, handlinger
- Ekspanderet række: alle spørgsmål + svar
- Bekræft (grøn) / Afvis (rød) knapper → bekræftelse modal

---

## Afgrænsning (ikke i scope)
- Fil-upload i ansøgninger
- Notifikation til captain ved afvisning (kun ved bekræftelse)
- Automatisk tilmelding til sæson ved bekræftelse
- Redigering af indsendt ansøgning
