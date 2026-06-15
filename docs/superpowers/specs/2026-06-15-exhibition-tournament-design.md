# Exhibition Tournament — Design Spec
**Dato:** 2026-06-15
**Projekt:** esportserien.dk
**Status:** Godkendt

---

## Overblik

Et exhibition-turneringssystem der giver admins mulighed for at oprette standalone bracket-turneringer (ikke liga) koblet på en sæson. Første template er "Kvalifikationsformat": BO1 i alle kampe op til finalen, som er BO3. Holdene seedes automatisk via gennemsnitlig FACEIT ELO eller manuelt af admin. Turneringen vises under "Stilling" som eneste visning for exhibition-sæsoner (ingen liga-tabel).

---

## Krav

- **Enten/eller:** En sæson er enten liga eller exhibition — aldrig begge
- **8 hold** (modulær arkitektur til fremtidig udvidelse)
- **Admin manuelt** — admin tilføjer hold til sæsonen og opretter turneringen
- **Seeding:** automatisk via avg FACEIT ELO (højest = seed 1) eller manuel omsortér
- **Fuldt CS2-integreret:** kampe startes via MatchZy/RCON, live scores via manager
- **Runde-scheduling:** admin angiver tidspunkt pr. runde (QF, SF, F)
- **Auto-propagering:** vinder rykker automatisk videre, næste kamp oprettes automatisk
- **Templates:** "Kvalifikationsformat (BO1 / BO3 finale)" — første template, modulær til flere

---

## Datamodel

### Schema-ændringer (migration)

```prisma
model TournamentRound {
  // ...eksisterende felter...
  matchFormat  String?   // per-runde format override; null = brug tournament.matchFormat
  scheduledAt  DateTime? // starttidspunkt for runden (QF, SF, F)
}
```

`Tournament.type` er allerede `String` (default `"LEAGUE"`). Ny værdi: `"EXHIBITION"` — ingen migration nødvendig.

### Template-logik

Når `template = "BO1_BO3_FINAL"` bruges ved oprettelse, sættes pr. runde efter bracket-generering:
- Alle runder: `matchFormat = "BO1"`
- Finalen (`isFinal = true`): `matchFormat = "BO3"`

### `scheduledAt` → `matchDate`

Når `propagateWinner` auto-opretter en Match i næste runde, sættes `matchDate = round.scheduledAt ?? null`.

---

## Backend

### Eksisterende filer der ændres

**`/root/esports-platform-main/api/src/tournament/tournament.service.ts`**

| Funktion | Ændring |
|----------|---------|
| `createTournament` | Accepterer `type?: string`, `template?: string`, `autoSeedByElo?: boolean`. Efter bracket-generering, hvis template = `"BO1_BO3_FINAL"`: sæt `matchFormat = "BO1"` på alle runder, `matchFormat = "BO3"` på finalen. |
| `propagateWinner` (privat) | Brug `round.matchFormat ?? tournament.matchFormat` ved oprettelse af Match. Include `round` i BracketMatch-query så `round.matchFormat` er tilgængeligt. |
| `generateMatchesForBracket` | Samme rettelse — per-runde matchFormat. Inkludér runden i bracketMatch-query. |

**`/root/esports-platform-main/api/src/matches/matches.service.ts`**

| Funktion | Ændring |
|----------|---------|
| `propagateBracketWinner` | Denne funktion kaldes fra `series_end` webhook (CS2-auto-path). Opdater på samme måde som `tournament.service.ts`'s `propagateWinner`: brug `round.matchFormat ?? tournament.matchFormat` ved oprettelse af næste Match. |

**`/root/esports-platform-main/api/src/tournament/tournament.controller.ts`**

Tilføj to nye endpoints.

### Nye endpoints

| Method | Path | Guard | Beskrivelse |
|--------|------|-------|-------------|
| `PATCH` | `/tournaments/:id/rounds` | Admin | Opdater `scheduledAt` og/eller `matchFormat` på én eller flere runder. Body: `{ rounds: [{ roundId: string; scheduledAt?: string; matchFormat?: string }] }` |
| `GET` | `/seasons/:id/teams/by-elo` | Ingen | Returnerer sæsonens hold sorteret efter avg `faceitElo` fra `TeamMember → User`. Hold uden ELO-data placeres sidst. |

### Auto-seed via FACEIT ELO

Når `autoSeedByElo = true` i `createTournament`:
1. Hent alle `TeamMember` for hvert hold i sæsonen inkl. `user.faceitElo`
2. Beregn avg ELO pr. hold (ignorer null-værdier; hold uden data avg = 0)
3. Sortér hold: højeste avg ELO → seed 1

---

## Frontend

### Ny fil: `/root/esport-web-main/app/admin/exhibition/page.tsx`

Dedikeret admin-side for exhibition-turneringer. 3-trins oprettelsesflow:

**Trin 1 — Turneringsinfo:**
- Turneringsnavn (input)
- Sæson-vælger (dropdown)
- Template-vælger: `"Kvalifikationsformat (BO1 / BO3 finale)"` *(eneste mulighed nu)*
- Antal hold: 8 (vises som info-badge, ikke redigerbar)

**Trin 2 — Seeding:**
- `"Auto-seed via FACEIT ELO"`-knap → kalder `GET /seasons/:id/teams/by-elo`, opdaterer listen
- Viser holdlogo + holdnavn + avg ELO
- Manuel omsortér via ↑/↓ pil-knapper
- Seed-nummer vises som `#1`, `#2` osv.

**Trin 3 — Tidsplan:**
- Tre `datetime-local` inputs: Kvartfinale, Semifinale, Finale
- Alle valgfrie
- Sættes via `PATCH /tournaments/:id/rounds` umiddelbart efter oprettelse (inden kampgenerering)

**Efter oprettelse — Bracket-admin (inline på siden):**
- Seeds-oversigt med swap via dropdown (eksisterende mønster)
- Runder med matches — viser hold, status, vinder-knapper
- Per-runde "Rediger tidspunkt"-knap (inline datetime-picker)
- `"Generer kampe"`-knap — opretter Match-records for alle READY bracket-matches. Skal klikkes *efter* tidsplanen er sat, så `matchDate` bliver korrekt.
- "Regenerer bracket"-knap
- "Slet turnering"-knap

### Ændret fil: `/root/esport-web-main/app/admin/page.tsx`

Tilføj i `links`-arrayet:
```typescript
{ href: "/admin/exhibition", title: "Exhibition-turnering", desc: "Opret og administrer standalone bracket-turneringer" },
```

### Ændret fil: `/root/esport-web-main/app/stilling/page.tsx`

**Exhibition-detektion:**
```
if (sæson har turnering med type === "EXHIBITION"):
  - Skjul Liga-tab og LeagueTable
  - Skjul standings-beregning
  - Vis BracketView direkte (ingen tab-vælger)
  - Vis vinder-banner når status === "FINISHED"
else:
  - Eksisterende Liga/Mesterskab/Nedrykning-flow uændret
```

Eksisterende `BracketView` og `BracketMatchCard` komponenter genbruges uden ændringer.

---

## Turneringsflow (end-to-end)

1. Admin opretter sæson (eksisterende flow)
2. Admin tilføjer hold til sæsonen via `/admin/season-teams` (eksisterende flow)
3. Admin går til `/admin/exhibition`:
   - Trin 1: navn, sæson, template
   - Trin 2: seeding (auto via ELO eller manuel) → `POST /tournaments` med `type="EXHIBITION"`
   - Backend genererer bracket og sætter per-runde matchFormat fra template
   - Trin 3: tidsplan → `PATCH /tournaments/:id/rounds` sætter `scheduledAt` pr. runde
4. Admin klikker "Generer kampe" → `POST /tournaments/:id/generate-matches` opretter Match-records for alle READY QF-matches med `matchDate = round.scheduledAt`
5. Kampe startes via eksisterende MatchZy/RCON flow
6. Når kamp afsluttes → `series_end` → `matches.service.ts propagateBracketWinner` → BracketMatch sættes som COMPLETED → næste BracketMatch opdateres med begge hold → ny Match oprettes automatisk med `matchFormat = round.matchFormat ?? tournament.matchFormat` og `matchDate = round.scheduledAt`
7. Stilling-siden viser bracket live via Socket.IO

---

## Afgrænsning (ikke i scope)

- Double elimination
- Mere end 8 hold (arkitektur understøtter det, men UI er ikke designet til det)
- Gruppeformat (round robin)
- Captain-tilmelding til exhibition (kun admin-oprettelse)
- Notifikationer til captains ved kampoprettelse
- Tredjepladsekamp
