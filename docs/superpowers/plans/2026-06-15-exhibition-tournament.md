# Exhibition Tournament Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tilføj exhibition-turneringer til esportserien.dk — standalone single elimination brackets koblet på en sæson, med per-runde matchformat (template: BO1/BO3 finale), per-runde scheduling og auto-propagering via CS2/MatchZy.

**Architecture:** Udvider det eksisterende Tournament/TournamentRound/BracketMatch-system med to nye felter på `TournamentRound` (`matchFormat`, `scheduledAt`), en ny `type="EXHIBITION"` value, og ny admin-side `/admin/exhibition/`. Stilling-siden opdateres til at detektere exhibition og skjule liga-tabellen. Eksisterende bracket-motor, propagerings-logik og MatchZy-integration genbruges.

**Tech Stack:** NestJS + Prisma + PostgreSQL (backend), Next.js 14 App Router + Tailwind CSS (frontend). SSH: `ssh -i ~/.ssh/id_ed25519 root@92.118.207.29`. Backend: `/root/esports-platform-main/api/`. Frontend: `/root/esport-web-main/`. Deploy: `cd /root/esport-prod && docker compose build <service> && docker compose up -d <service>`.

---

## Filer

| Handling | Fil |
|----------|-----|
| Modificér | `/root/esports-platform-main/api/prisma/schema.prisma` |
| Modificér | `/root/esports-platform-main/api/src/tournament/tournament.service.ts` |
| Modificér | `/root/esports-platform-main/api/src/tournament/tournament.controller.ts` |
| Modificér | `/root/esports-platform-main/api/src/matches/matches.service.ts` |
| Modificér | `/root/esports-platform-main/api/src/seasons/seasons.service.ts` |
| Modificér | `/root/esports-platform-main/api/src/seasons/seasons.controller.ts` |
| Opret | `/root/esport-web-main/app/admin/exhibition/page.tsx` |
| Modificér | `/root/esport-web-main/app/admin/page.tsx` |
| Modificér | `/root/esport-web-main/app/stilling/page.tsx` |

---

## Task 1: Schema Migration

**Filer:**
- Modificér: `/root/esports-platform-main/api/prisma/schema.prisma`

- [ ] **Step 1: SSH til backend-serveren**

```bash
ssh -i ~/.ssh/id_ed25519 root@92.118.207.29
```

- [ ] **Step 2: Tilføj felter til TournamentRound i schema.prisma**

Find `model TournamentRound` i `/root/esports-platform-main/api/prisma/schema.prisma`. Tilføj to linjer efter `isLosersBracket Boolean @default(false)`:

```prisma
  matchFormat     String?   // per-runde format override; null = brug tournament.matchFormat
  scheduledAt     DateTime? // admin sætter starttidspunkt pr. runde (QF, SF, F)
```

Resultatet skal se sådan ud:
```prisma
model TournamentRound {
  id           String          @id @default(uuid())
  tournamentId String
  roundNumber  Int
  name         String
  isFinal      Boolean         @default(false)
  isLosersBracket Boolean      @default(false)
  matchFormat     String?
  scheduledAt     DateTime?

  tournament   Tournament      @relation(fields: [tournamentId], references: [id])
  matches      BracketMatch[]

  @@unique([tournamentId, roundNumber, isLosersBracket])
}
```

- [ ] **Step 3: Kør migration**

```bash
cd /root/esports-platform-main/api
DATABASE_URL="postgresql://esport:supersecret@localhost:5432/esportdb" npx prisma migrate dev --name add_tournament_round_schedule
```

Hvis `localhost` ikke virker (postgres er i Docker):
```bash
DATABASE_URL="postgresql://esport:supersecret@$(docker inspect esport-prod-postgres-1 --format '{{.NetworkSettings.Networks.esport_prod_default.IPAddress}}'):5432/esportdb" npx prisma migrate dev --name add_tournament_round_schedule
```

Forventet output: `The following migration(s) have been applied: ...add_tournament_round_schedule`

- [ ] **Step 4: Verificér kolonner**

```bash
docker exec esport-prod-postgres-1 psql -U esport -d esportdb -c '\d "TournamentRound"' | grep -E 'matchFormat|scheduledAt'
```

Forventet output:
```
 matchFormat  | text                        |           |          |
 scheduledAt  | timestamp without time zone |           |          |
```

- [ ] **Step 5: Commit**

```bash
cd /root/esports-platform-main
git add api/prisma/schema.prisma api/prisma/migrations/
git commit -m "feat(db): add matchFormat and scheduledAt to TournamentRound"
```

---

## Task 2: Tournament Service — createTournament med type + template

**Filer:**
- Modificér: `/root/esports-platform-main/api/src/tournament/tournament.service.ts`

- [ ] **Step 1: Opdater funktionssignaturen for createTournament**

Find `async createTournament(data: {` i `tournament.service.ts`. Erstat hele signaturen (de curly-bracket-linjer indtil `}) {`):

```typescript
async createTournament(data: {
  seasonId: string;
  name: string;
  format: string;
  matchFormat: string;
  teamCount: number;
  startDate?: string;
  seedOrder?: string[];
  type?: string;
  template?: string;
}) {
```

- [ ] **Step 2: Tilføj type-opdatering og template-kald efter generateBracket**

Find denne linje i `createTournament`:
```typescript
  await this.generateBracket(tournament.id, data.format, orderedTeams.length);

  return this.getTournament(tournament.id);
```

Erstat med:
```typescript
  await this.generateBracket(tournament.id, data.format, orderedTeams.length);

  if (data.template) {
    await this.applyTemplate(tournament.id, data.template);
  }

  if (data.type && data.type !== 'LEAGUE') {
    await this.prisma.$executeRawUnsafe(
      `UPDATE "Tournament" SET "type" = $1 WHERE id = $2`,
      data.type, tournament.id
    );
  }

  return this.getTournament(tournament.id);
```

- [ ] **Step 3: Tilføj privat applyTemplate metode**

Tilføj følgende private metode i klassen, lige før `private generateSingleElimination`:

```typescript
  private async applyTemplate(tournamentId: string, template: string) {
    if (template !== 'BO1_BO3_FINAL') return;
    const rounds = await this.prisma.tournamentRound.findMany({
      where: { tournamentId },
    });
    for (const round of rounds) {
      await this.prisma.tournamentRound.update({
        where: { id: round.id },
        data: { matchFormat: round.isFinal ? 'BO3' : 'BO1' },
      });
    }
  }
```

- [ ] **Step 4: Commit**

```bash
cd /root/esports-platform-main
git add api/src/tournament/tournament.service.ts
git commit -m "feat(tournament): add type and template support to createTournament"
```

---

## Task 3: Tournament Service — per-runde matchFormat i propagateWinner og generateMatchesForBracket

**Filer:**
- Modificér: `/root/esports-platform-main/api/src/tournament/tournament.service.ts`

- [ ] **Step 1: Ret propagateWinner til at bruge per-runde matchFormat**

Find denne blok i den private `propagateWinner` metode (inde i `if (newStatus === 'READY' && !nextMatch.matchId)` blokken):

```typescript
        if (round) {
          const match = await this.prisma.match.create({
            data: {
              seasonId: round.tournament.seasonId,
              team1Id: nextTeam1Id,
              team2Id: nextTeam2Id,
              matchFormat: round.tournament.matchFormat,
              matchDate: (round.tournament as any).startDate ?? null,
              status: 'SCHEDULED',
            } as any
          });
```

Erstat med (samme indrykning):
```typescript
        if (round) {
          const match = await this.prisma.match.create({
            data: {
              seasonId: round.tournament.seasonId,
              team1Id: nextTeam1Id,
              team2Id: nextTeam2Id,
              matchFormat: ((round as any).matchFormat ?? round.tournament.matchFormat) as any,
              matchDate: (round as any).scheduledAt ?? (round.tournament as any).startDate ?? null,
              status: 'SCHEDULED',
            } as any
          });
```

**Forklaring:** `round` her er allerede næste rundes data (hentet via `nextMatch.roundId`), så `round.matchFormat` er korrekt — det er den runde kampen spilles i.

- [ ] **Step 2: Ret generateMatchesForBracket til at inkludere runde og bruge per-runde matchFormat**

Find `async generateMatchesForBracket(tournamentId: string)`. Find denne `findMany` kald:

```typescript
    const readyMatches = await this.prisma.bracketMatch.findMany({
      where: {
        round: { tournamentId },
        status: 'READY',
        matchId: null,
        team1Id: { not: null },
        team2Id: { not: null },
      },
    });
```

Erstat med:
```typescript
    const readyMatches = await this.prisma.bracketMatch.findMany({
      where: {
        round: { tournamentId },
        status: 'READY',
        matchId: null,
        team1Id: { not: null },
        team2Id: { not: null },
      },
      include: { round: true },
    });
```

Find derefter inde i `for (const bm of readyMatches)` løkken:

```typescript
      const match = await this.prisma.match.create({
        data: {
          seasonId: tournament.seasonId,
          team1Id: bm.team1Id!,
          team2Id: bm.team2Id!,
          matchDate: tournament.startDate ?? null,
          matchFormat: tournament.matchFormat,
          status: 'SCHEDULED',
        } as any
      });
```

Erstat med:
```typescript
      const match = await this.prisma.match.create({
        data: {
          seasonId: tournament.seasonId,
          team1Id: bm.team1Id!,
          team2Id: bm.team2Id!,
          matchDate: (bm.round as any).scheduledAt ?? tournament.startDate ?? null,
          matchFormat: ((bm.round as any).matchFormat ?? tournament.matchFormat) as any,
          status: 'SCHEDULED',
        } as any
      });
```

- [ ] **Step 3: Commit**

```bash
cd /root/esports-platform-main
git add api/src/tournament/tournament.service.ts
git commit -m "feat(tournament): use per-round matchFormat and scheduledAt when creating matches"
```

---

## Task 4: Matches Service — per-runde matchFormat i propagateBracketWinner

**Filer:**
- Modificér: `/root/esports-platform-main/api/src/matches/matches.service.ts`

- [ ] **Step 1: Ret propagateBracketWinner til at bruge næste rundes matchFormat**

Find `async propagateBracketWinner` (linje ~485). Inde i den `if (team1Set && team2Set && !nextMatch.matchId)` blok, find:

```typescript
            // Find tournament for at få matchFormat og startDate
            const round = await this.prisma.tournamentRound.findUnique({
              where: { id: bracketMatch.roundId },
              include: { tournament: true }
            });

            const newMatch = await this.prisma.match.create({
              data: {
                seasonId: round!.tournament.seasonId,
                team1Id: nextTeam1Id,
                team2Id: nextTeam2Id,
                matchFormat: round!.tournament.matchFormat,
                matchDate: (round!.tournament as any).startDate ?? null,
                status: 'SCHEDULED',
              } as any
            });
```

Erstat med:
```typescript
            // Hent NÆSTE rundes data for korrekt matchFormat og scheduledAt
            const nextRound = await this.prisma.tournamentRound.findUnique({
              where: { id: nextMatch.roundId },
              include: { tournament: true }
            });

            const newMatch = await this.prisma.match.create({
              data: {
                seasonId: nextRound!.tournament.seasonId,
                team1Id: nextTeam1Id,
                team2Id: nextTeam2Id,
                matchFormat: ((nextRound as any)!.matchFormat ?? nextRound!.tournament.matchFormat) as any,
                matchDate: (nextRound as any)!.scheduledAt ?? (nextRound!.tournament as any).startDate ?? null,
                status: 'SCHEDULED',
              } as any
            });
```

**Forklaring:** Den forrige kode brugte `bracketMatch.roundId` (den runde der netop er afsluttet), men matchFormat skal komme fra `nextMatch.roundId` (den runde den nye kamp spilles i).

- [ ] **Step 2: Commit**

```bash
cd /root/esports-platform-main
git add api/src/matches/matches.service.ts
git commit -m "fix(matches): use next round's matchFormat and scheduledAt in bracket propagation"
```

---

## Task 5: Nye endpoints — updateRoundSchedules + getTeamsByElo

**Filer:**
- Modificér: `/root/esports-platform-main/api/src/tournament/tournament.service.ts`
- Modificér: `/root/esports-platform-main/api/src/tournament/tournament.controller.ts`
- Modificér: `/root/esports-platform-main/api/src/seasons/seasons.service.ts`
- Modificér: `/root/esports-platform-main/api/src/seasons/seasons.controller.ts`

- [ ] **Step 1: Tilføj updateRoundSchedules til tournament.service.ts**

Tilføj følgende metode i `TournamentService` klassen, fx efter `updateSeeds`:

```typescript
  async updateRoundSchedules(tournamentId: string, rounds: { roundId: string; scheduledAt?: string; matchFormat?: string }[]) {
    for (const r of rounds) {
      const updateData: any = {};
      if (r.scheduledAt !== undefined) updateData.scheduledAt = r.scheduledAt ? new Date(r.scheduledAt) : null;
      if (r.matchFormat !== undefined) updateData.matchFormat = r.matchFormat;
      if (Object.keys(updateData).length === 0) continue;
      await this.prisma.tournamentRound.update({
        where: { id: r.roundId },
        data: updateData,
      });
    }
    return this.getTournament(tournamentId);
  }
```

- [ ] **Step 2: Tilføj PATCH /tournaments/:id/rounds til tournament.controller.ts**

Tilføj følgende endpoint i `TournamentController` klassen (fx efter `updateSeeds` endpointet):

```typescript
  // Admin: Opdater scheduledAt og/eller matchFormat pr. runde
  @Patch(':id/rounds')
  @UseGuards(JwtAuthGuard, AdminGuard)
  updateRoundSchedules(
    @Param('id') id: string,
    @Body() body: { rounds: { roundId: string; scheduledAt?: string; matchFormat?: string }[] },
  ) {
    return this.tournamentService.updateRoundSchedules(id, body.rounds);
  }
```

Sørg for at `Patch` er importeret fra `@nestjs/common` øverst i filen. Det er sandsynligvis allerede der.

- [ ] **Step 3: Tilføj getTeamsByElo til seasons.service.ts**

Tilføj følgende metode i `SeasonsService` klassen:

```typescript
  async getTeamsByElo(seasonId: string) {
    const seasonTeams = await this.prisma.seasonTeam.findMany({
      where: { seasonId },
      include: {
        team: {
          include: {
            members: {
              include: { user: { select: { faceitElo: true } } },
            },
          },
        },
      },
    });

    return seasonTeams.map(st => {
      const elos = st.team.members
        .map((m: any) => m.user.faceitElo)
        .filter((e: any): e is number => e !== null);
      const avgElo = elos.length > 0
        ? Math.round(elos.reduce((a: number, b: number) => a + b, 0) / elos.length)
        : 0;
      return {
        id: st.team.id,
        name: st.team.name,
        slug: st.team.slug,
        logoUrl: st.team.logoUrl ?? null,
        avgElo,
      };
    }).sort((a: any, b: any) => b.avgElo - a.avgElo);
  }
```

- [ ] **Step 4: Tilføj GET /seasons/:id/teams/by-elo til seasons.controller.ts**

Find `@Get(":id/teams")` i `seasons.controller.ts`. Tilføj følgende endpoint FØR den eksisterende `@Get(":id/teams")` (vigtig rækkefølge — specifikt path skal stå før generelt):

```typescript
  @Get(':id/teams/by-elo')
  getTeamsByElo(@Param('id') id: string) {
    return this.seasonsService.getTeamsByElo(id);
  }
```

- [ ] **Step 5: Commit**

```bash
cd /root/esports-platform-main
git add api/src/tournament/tournament.service.ts api/src/tournament/tournament.controller.ts api/src/seasons/seasons.service.ts api/src/seasons/seasons.controller.ts
git commit -m "feat(tournament): add updateRoundSchedules endpoint and getTeamsByElo endpoint"
```

---

## Task 6: Backend Build + Deploy

**Filer:** Ingen kodeændringer

- [ ] **Step 1: Byg og verificér**

```bash
cd /root/esports-platform-main/api
npm run build 2>&1 | tail -20
```

Forventet output: `Successfully compiled` eller `webpack compiled successfully`. Ingen TypeScript-fejl. Hvis der er fejl, ret dem og commit inden du fortsætter.

- [ ] **Step 2: Deploy**

```bash
cd /root/esport-prod
docker compose build backend && docker compose up -d backend
```

- [ ] **Step 3: Verificér endpoints**

Vent ~20 sekunder, kør:
```bash
# Tjek at serveren kører
curl -s http://localhost:3000/seasons | head -c 100

# Test by-elo endpoint (brug en rigtig sæson-id fra DB)
SEASON_ID=$(docker exec esport-prod-postgres-1 psql -U esport -d esportdb -t -c "SELECT id FROM \"Season\" LIMIT 1;" | tr -d ' \n')
curl -s "http://localhost:3000/seasons/${SEASON_ID}/teams/by-elo" | head -c 200
```

Forventet: JSON-array sorteret efter avgElo.

---

## Task 7: Admin Exhibition Side

**Filer:**
- Opret: `/root/esport-web-main/app/admin/exhibition/page.tsx`

- [ ] **Step 1: Opret mappe og fil**

```bash
mkdir -p /root/esport-web-main/app/admin/exhibition
```

Opret `/root/esport-web-main/app/admin/exhibition/page.tsx`:

```tsx
"use client";

import { useEffect, useState } from "react";

interface TeamWithElo {
  id: string;
  name: string;
  slug: string;
  logoUrl: string | null;
  avgElo: number;
}

interface Round {
  id: string;
  name: string;
  roundNumber: number;
  isFinal: boolean;
  matchFormat: string | null;
  scheduledAt: string | null;
  matches: BracketMatch[];
}

interface BracketMatch {
  id: string;
  position: number;
  status: string;
  winnerId: string | null;
  team1: { id: string; name: string; logoUrl: string | null } | null;
  team2: { id: string; name: string; logoUrl: string | null } | null;
  winner: { id: string; name: string } | null;
  match: { id: string; status: string } | null;
}

interface Tournament {
  id: string;
  name: string;
  type: string;
  status: string;
  matchFormat: string;
  seeds: { seed: number; teamId: string; team: { id: string; name: string; logoUrl: string | null } }[];
  rounds: Round[];
}

const TEMPLATES = [
  { value: "BO1_BO3_FINAL", label: "Kvalifikationsformat (BO1 / BO3 finale)" },
];

export default function AdminExhibitionPage() {
  const [seasons, setSeasons] = useState<any[]>([]);
  const [seasonId, setSeasonId] = useState("");
  const [tournament, setTournament] = useState<Tournament | null>(null);
  const [loading, setLoading] = useState(false);
  const [msg, setMsg] = useState<{ text: string; ok: boolean } | null>(null);

  // Oprettelse flow
  const [step, setStep] = useState<1 | 2 | 3 | null>(null);
  const [form, setForm] = useState({ name: "", template: "BO1_BO3_FINAL" });
  const [teams, setTeams] = useState<TeamWithElo[]>([]);
  const [seedOrder, setSeedOrder] = useState<string[]>([]);
  const [eloLoading, setEloLoading] = useState(false);
  const [schedules, setSchedules] = useState<Record<string, string>>({});

  // Runde-redigering
  const [editingRound, setEditingRound] = useState<string | null>(null);

  const getToken = () => localStorage.getItem("token");

  const showMsg = (text: string, ok: boolean) => {
    setMsg({ text, ok });
    setTimeout(() => setMsg(null), 4000);
  };

  useEffect(() => {
    fetch("https://api.esportserien.dk/seasons")
      .then(r => r.json())
      .then(data => {
        setSeasons(Array.isArray(data) ? data : []);
        const s = Array.isArray(data) ? data[0] : null;
        if (s) setSeasonId(s.id);
      });
  }, []);

  useEffect(() => {
    if (!seasonId) return;
    setTournament(null);
    setStep(null);
    fetch(`https://api.esportserien.dk/tournaments/season/${seasonId}/all`)
      .then(r => r.json())
      .then(data => {
        const all = Array.isArray(data) ? data : [];
        const ex = all.find((t: any) => t.type === "EXHIBITION") ?? null;
        setTournament(ex);
      })
      .catch(() => setTournament(null));
  }, [seasonId]);

  const loadTeams = async () => {
    setEloLoading(true);
    const data = await fetch(`https://api.esportserien.dk/seasons/${seasonId}/teams/by-elo`)
      .then(r => r.json()).catch(() => []);
    const teamList: TeamWithElo[] = Array.isArray(data) ? data : [];
    setTeams(teamList);
    setSeedOrder(teamList.map(t => t.id));
    setEloLoading(false);
  };

  const goToStep2 = async () => {
    if (!form.name.trim()) { showMsg("Angiv et turneringsnavn", false); return; }
    await loadTeams();
    setStep(2);
  };

  const moveTeam = (idx: number, dir: -1 | 1) => {
    const next = idx + dir;
    if (next < 0 || next >= seedOrder.length) return;
    const s = [...seedOrder];
    [s[idx], s[next]] = [s[next], s[idx]];
    setSeedOrder(s);
  };

  const createTournament = async () => {
    setLoading(true);
    const res = await fetch("https://api.esportserien.dk/tournaments", {
      method: "POST",
      headers: { "Content-Type": "application/json", Authorization: `Bearer ${getToken()}` },
      body: JSON.stringify({
        seasonId,
        name: form.name,
        format: "SINGLE_ELIMINATION",
        matchFormat: "BO1",
        teamCount: seedOrder.length,
        type: "EXHIBITION",
        template: form.template,
        seedOrder,
      }),
    });
    setLoading(false);
    if (!res.ok) {
      const d = await res.json().catch(() => ({}));
      showMsg(d.message ?? "Fejl ved oprettelse", false);
      return;
    }
    const t: Tournament = await res.json();
    setTournament(t);
    // Initialiser schedules fra rundenavne
    const initial: Record<string, string> = {};
    for (const r of t.rounds) initial[r.id] = r.scheduledAt ? r.scheduledAt.slice(0, 16) : "";
    setSchedules(initial);
    setStep(3);
  };

  const saveSchedules = async () => {
    if (!tournament) return;
    setLoading(true);
    const rounds = tournament.rounds.map(r => ({
      roundId: r.id,
      ...(schedules[r.id] ? { scheduledAt: new Date(schedules[r.id]).toISOString() } : {}),
    }));
    const res = await fetch(`https://api.esportserien.dk/tournaments/${tournament.id}/rounds`, {
      method: "PATCH",
      headers: { "Content-Type": "application/json", Authorization: `Bearer ${getToken()}` },
      body: JSON.stringify({ rounds }),
    });
    setLoading(false);
    if (res.ok) {
      const updated: Tournament = await res.json();
      setTournament(updated);
      setStep(null);
      showMsg("Turnering oprettet og tidsplan gemt!", true);
    } else {
      showMsg("Fejl ved gemning af tidsplan", false);
    }
  };

  const generateMatches = async () => {
    if (!tournament) return;
    const res = await fetch(`https://api.esportserien.dk/tournaments/${tournament.id}/generate-matches`, {
      method: "POST",
      headers: { Authorization: `Bearer ${getToken()}` },
    });
    const data = await res.json();
    if (res.ok) showMsg(`${data.created} kampe genereret!`, true);
    else showMsg(data.message ?? "Fejl", false);
  };

  const regenerate = async () => {
    if (!tournament) return;
    if (!confirm("Nulstil og regenerer bracket? Alle resultater slettes.")) return;
    const res = await fetch(`https://api.esportserien.dk/tournaments/${tournament.id}/regenerate`, {
      method: "POST",
      headers: { Authorization: `Bearer ${getToken()}` },
    });
    if (res.ok) { setTournament(await res.json()); showMsg("Bracket regenereret", true); }
    else showMsg("Fejl", false);
  };

  const deleteTournament = async () => {
    if (!tournament) return;
    if (!confirm("Slet turneringen? Kan ikke fortrydes.")) return;
    const res = await fetch(`https://api.esportserien.dk/tournaments/${tournament.id}`, {
      method: "DELETE",
      headers: { Authorization: `Bearer ${getToken()}` },
    });
    if (res.ok) { setTournament(null); showMsg("Turnering slettet", true); }
    else showMsg("Fejl ved sletning", false);
  };

  const setWinner = async (bracketMatchId: string, winnerId: string) => {
    if (!tournament) return;
    const res = await fetch(`https://api.esportserien.dk/tournaments/${bracketMatchId}/winner`, {
      method: "POST",
      headers: { "Content-Type": "application/json", Authorization: `Bearer ${getToken()}` },
      body: JSON.stringify({ winnerId }),
    });
    if (res.ok) { setTournament(await res.json()); showMsg("Vinder sat!", true); }
    else showMsg("Fejl", false);
  };

  const saveRoundSchedule = async (roundId: string) => {
    if (!tournament) return;
    const res = await fetch(`https://api.esportserien.dk/tournaments/${tournament.id}/rounds`, {
      method: "PATCH",
      headers: { "Content-Type": "application/json", Authorization: `Bearer ${getToken()}` },
      body: JSON.stringify({
        rounds: [{ roundId, ...(schedules[roundId] ? { scheduledAt: new Date(schedules[roundId]).toISOString() } : { scheduledAt: null }) }],
      }),
    });
    if (res.ok) {
      setTournament(await res.json());
      setEditingRound(null);
      showMsg("Tidspunkt gemt", true);
    } else showMsg("Fejl", false);
  };

  const updateSeed = async (teamId: string, newSeed: number) => {
    if (!tournament) return;
    const currentSeed = tournament.seeds.find(s => s.teamId === teamId)?.seed;
    const swapTeam = tournament.seeds.find(s => s.seed === newSeed && s.teamId !== teamId);
    const seeds = tournament.seeds.map(s => {
      if (s.teamId === teamId) return { teamId: s.teamId, seed: newSeed };
      if (swapTeam && s.teamId === swapTeam.teamId) return { teamId: s.teamId, seed: currentSeed! };
      return { teamId: s.teamId, seed: s.seed };
    });
    const res = await fetch(`https://api.esportserien.dk/tournaments/${tournament.id}/seeds`, {
      method: "PUT",
      headers: { "Content-Type": "application/json", Authorization: `Bearer ${getToken()}` },
      body: JSON.stringify({ seeds }),
    });
    if (res.ok) showMsg("Seed byttet — klik 'Regenerer bracket' for at anvende", true);
    else showMsg("Fejl", false);
  };

  const formatDT = (iso: string | null) => {
    if (!iso) return "—";
    return new Date(iso).toLocaleString("da-DK", { dateStyle: "short", timeStyle: "short" });
  };

  return (
    <div className="max-w-4xl mx-auto space-y-6">
      <div className="flex items-center justify-between flex-wrap gap-4">
        <div>
          <h1 className="text-3xl font-extrabold">Exhibition Turnering</h1>
          <p className="text-sm opacity-40 mt-1">Standalone bracket-turnering koblet på en sæson</p>
        </div>
        <select
          value={seasonId}
          onChange={e => setSeasonId(e.target.value)}
          className="bg-[var(--bg-secondary)] border border-[var(--border)] px-4 py-2 rounded-xl text-sm outline-none"
        >
          {seasons.map(s => <option key={s.id} value={s.id}>{s.name}</option>)}
        </select>
      </div>

      {msg && (
        <div className={`px-4 py-3 rounded-xl text-sm font-semibold border ${msg.ok ? "bg-green-900/30 text-green-400 border-green-800" : "bg-red-900/30 text-red-400 border-red-800"}`}>
          {msg.text}
        </div>
      )}

      {/* ── INGEN TURNERING ── */}
      {!tournament && step === null && (
        <div className="bg-[var(--bg-primary)] rounded-2xl border border-[var(--border)] p-10 text-center space-y-4">
          <div className="text-4xl opacity-20">🏆</div>
          <div>
            <h2 className="font-bold text-lg">Ingen exhibition-turnering for denne sæson</h2>
            <p className="text-sm opacity-50 mt-1">Opret en turnering for at generere et bracket</p>
          </div>
          <button
            onClick={() => setStep(1)}
            className="bg-[#c23c84] hover:brightness-110 px-8 py-3 rounded-xl font-bold transition"
          >
            Opret turnering
          </button>
        </div>
      )}

      {/* ── TRIN 1: INFO ── */}
      {step === 1 && (
        <div className="bg-[var(--bg-primary)] rounded-2xl border border-[var(--border)] p-8 space-y-6">
          <div className="text-center">
            <div className="text-xs opacity-50 mb-1">Trin 1 af 3 — Turneringsinfo</div>
            <h2 className="text-lg font-bold">Grundlæggende info</h2>
          </div>
          <div className="max-w-md mx-auto space-y-4">
            <div>
              <label className="text-xs opacity-50 uppercase tracking-wider">Turneringsnavn *</label>
              <input
                value={form.name}
                onChange={e => setForm({ ...form, name: e.target.value })}
                placeholder="f.eks. Spring Invitational 2026"
                className="w-full mt-1 bg-[var(--bg-secondary)] border border-[var(--border)] px-4 py-2.5 rounded-xl text-sm outline-none focus:border-[#c23c84] transition"
              />
            </div>
            <div>
              <label className="text-xs opacity-50 uppercase tracking-wider">Template</label>
              <select
                value={form.template}
                onChange={e => setForm({ ...form, template: e.target.value })}
                className="w-full mt-1 bg-[var(--bg-secondary)] border border-[var(--border)] px-4 py-2.5 rounded-xl text-sm outline-none"
              >
                {TEMPLATES.map(t => <option key={t.value} value={t.value}>{t.label}</option>)}
              </select>
            </div>
            <div className="bg-[var(--bg-secondary)] rounded-xl px-4 py-3 text-xs opacity-60">
              <span className="font-semibold">8 hold</span> · Single Elimination · Kvartfinale → Semifinale → Finale
            </div>
            <div className="flex gap-3">
              <button
                onClick={goToStep2}
                className="flex-1 bg-[#c23c84] hover:brightness-110 py-2.5 rounded-xl text-sm font-bold transition"
              >
                Næste: Seeding →
              </button>
              <button onClick={() => setStep(null)} className="px-4 bg-[var(--bg-secondary)] rounded-xl text-sm transition">
                Annuller
              </button>
            </div>
          </div>
        </div>
      )}

      {/* ── TRIN 2: SEEDING ── */}
      {step === 2 && (
        <div className="bg-[var(--bg-primary)] rounded-2xl border border-[var(--border)] p-8 space-y-6">
          <div className="text-center">
            <div className="text-xs opacity-50 mb-1">Trin 2 af 3 — Seeding</div>
            <h2 className="text-lg font-bold">Seed holdene</h2>
            <p className="text-xs opacity-50 mt-1">Seed 1 møder seed 8 i kvartfinalen. Brug ↑/↓ til manuel justering.</p>
          </div>
          <div className="max-w-md mx-auto space-y-3">
            <button
              onClick={loadTeams}
              disabled={eloLoading}
              className="w-full py-2.5 bg-[var(--bg-secondary)] border border-[var(--border)] rounded-xl text-sm font-semibold hover:border-[#c23c84]/50 transition disabled:opacity-50"
            >
              {eloLoading ? "Henter ELO..." : "↻ Auto-seed via FACEIT ELO"}
            </button>
            {teams.length === 0 && (
              <div className="text-center text-sm opacity-40 py-4">Klik 'Auto-seed' for at hente holdene, eller tilføj hold til sæsonen først</div>
            )}
            {seedOrder.map((teamId, idx) => {
              const team = teams.find(t => t.id === teamId);
              if (!team) return null;
              return (
                <div key={teamId} className="flex items-center gap-3 bg-[var(--bg-secondary)] px-4 py-3 rounded-xl border border-[var(--border)]">
                  <span className="text-sm font-bold text-[#e05aa0] w-6 flex-shrink-0">#{idx + 1}</span>
                  <div className="w-8 h-8 rounded-lg bg-[var(--bg-primary)] overflow-hidden flex-shrink-0">
                    {team.logoUrl
                      ? <img src={`https://api.esportserien.dk${team.logoUrl}`} className="w-full h-full object-cover" alt="" />
                      : <div className="w-full h-full flex items-center justify-center text-xs font-bold text-[#e05aa0]">{team.name[0]}</div>
                    }
                  </div>
                  <span className="flex-1 text-sm font-semibold">{team.name}</span>
                  <span className="text-xs opacity-40 mr-2">{team.avgElo > 0 ? `ELO ${team.avgElo}` : "—"}</span>
                  <div className="flex gap-1">
                    <button onClick={() => moveTeam(idx, -1)} disabled={idx === 0}
                      className="px-2 py-1 bg-[var(--bg-primary)] rounded-lg text-xs disabled:opacity-20 hover:bg-[var(--border)] transition">↑</button>
                    <button onClick={() => moveTeam(idx, 1)} disabled={idx === seedOrder.length - 1}
                      className="px-2 py-1 bg-[var(--bg-primary)] rounded-lg text-xs disabled:opacity-20 hover:bg-[var(--border)] transition">↓</button>
                  </div>
                </div>
              );
            })}
            <div className="flex gap-3 pt-2">
              <button
                onClick={createTournament}
                disabled={loading || seedOrder.length < 2}
                className="flex-1 bg-[#c23c84] hover:brightness-110 py-2.5 rounded-xl text-sm font-bold transition disabled:opacity-50"
              >
                {loading ? "Opretter..." : "Opret og generer bracket →"}
              </button>
              <button onClick={() => setStep(1)} className="px-4 bg-[var(--bg-secondary)] rounded-xl text-sm transition">
                Tilbage
              </button>
            </div>
          </div>
        </div>
      )}

      {/* ── TRIN 3: TIDSPLAN ── */}
      {step === 3 && tournament && (
        <div className="bg-[var(--bg-primary)] rounded-2xl border border-[var(--border)] p-8 space-y-6">
          <div className="text-center">
            <div className="text-xs opacity-50 mb-1">Trin 3 af 3 — Tidsplan</div>
            <h2 className="text-lg font-bold">Angiv tidspunkter</h2>
            <p className="text-xs opacity-50 mt-1">Valgfrit. Kan ændres senere.</p>
          </div>
          <div className="max-w-md mx-auto space-y-4">
            {tournament.rounds.map(r => (
              <div key={r.id}>
                <label className="text-xs opacity-50 uppercase tracking-wider">{r.name} {r.isFinal ? "(BO3)" : "(BO1)"}</label>
                <input
                  type="datetime-local"
                  value={schedules[r.id] ?? ""}
                  onChange={e => setSchedules(prev => ({ ...prev, [r.id]: e.target.value }))}
                  className="w-full mt-1 bg-[var(--bg-secondary)] border border-[var(--border)] px-4 py-2.5 rounded-xl text-sm outline-none focus:border-[#c23c84] transition"
                />
              </div>
            ))}
            <div className="flex gap-3 pt-2">
              <button
                onClick={saveSchedules}
                disabled={loading}
                className="flex-1 bg-[#c23c84] hover:brightness-110 py-2.5 rounded-xl text-sm font-bold transition disabled:opacity-50"
              >
                {loading ? "Gemmer..." : "Gem tidsplan og afslut →"}
              </button>
              <button onClick={() => { setStep(null); }} className="px-4 bg-[var(--bg-secondary)] rounded-xl text-sm transition">
                Spring over
              </button>
            </div>
          </div>
        </div>
      )}

      {/* ── BRACKET ADMIN ── */}
      {tournament && step === null && (
        <div className="space-y-6">
          {/* Info */}
          <div className="bg-[var(--bg-primary)] rounded-2xl border border-[var(--border)] p-5 flex items-center justify-between flex-wrap gap-4">
            <div>
              <h2 className="font-bold text-lg">{tournament.name}</h2>
              <div className="flex gap-2 mt-1 flex-wrap">
                <span className="text-xs bg-[#c23c84]/20 text-[#e05aa0] px-2 py-1 rounded-lg border border-[#c23c84]/30 font-semibold">EXHIBITION</span>
                <span className="text-xs bg-[var(--bg-secondary)] px-2 py-1 rounded-lg opacity-60">Single Elimination</span>
                <span className="text-xs bg-[var(--bg-secondary)] px-2 py-1 rounded-lg opacity-60">{tournament.seeds.length} hold</span>
                <span className={`text-xs px-2 py-1 rounded-lg font-semibold ${tournament.status === 'FINISHED' ? 'bg-green-900/30 text-green-400' : 'bg-yellow-900/30 text-yellow-400'}`}>
                  {tournament.status}
                </span>
              </div>
            </div>
            <div className="flex gap-2 flex-wrap">
              <button onClick={generateMatches} className="bg-green-900/30 hover:bg-green-900/50 text-green-400 px-4 py-2 rounded-xl text-sm transition font-semibold">
                Generer kampe
              </button>
              <button onClick={regenerate} className="bg-orange-900/30 hover:bg-orange-900/50 text-orange-400 px-4 py-2 rounded-xl text-sm transition">
                Regenerer bracket
              </button>
              <button onClick={deleteTournament} className="bg-red-900/30 hover:bg-red-900/50 text-red-400 px-4 py-2 rounded-xl text-sm transition">
                Slet
              </button>
            </div>
          </div>

          {/* Seeds */}
          <div className="bg-[var(--bg-primary)] rounded-2xl border border-[var(--border)] overflow-hidden">
            <div className="px-5 py-3 border-b border-[var(--border)]">
              <span className="text-sm font-semibold opacity-50 uppercase tracking-wider">Seeds</span>
            </div>
            <div className="divide-y divide-[var(--border)]">
              {tournament.seeds.map(s => (
                <div key={s.teamId} className="flex items-center gap-4 px-5 py-3">
                  <span className="text-sm font-bold text-[#e05aa0] w-6">#{s.seed}</span>
                  <div className="w-8 h-8 rounded-lg bg-[var(--bg-secondary)] overflow-hidden">
                    {s.team.logoUrl
                      ? <img src={`https://api.esportserien.dk${s.team.logoUrl}`} className="w-full h-full object-cover" alt="" />
                      : <div className="w-full h-full flex items-center justify-center text-xs font-bold text-[#e05aa0]">{s.team.name[0]}</div>
                    }
                  </div>
                  <span className="flex-1 text-sm font-semibold">{s.team.name}</span>
                  <select
                    defaultValue={s.seed}
                    onChange={e => updateSeed(s.teamId, parseInt(e.target.value))}
                    className="bg-[var(--bg-secondary)] border border-[var(--border)] px-2 py-1 rounded-lg text-xs outline-none"
                  >
                    {tournament.seeds.map((_, i) => (
                      <option key={i} value={i + 1}>Seed {i + 1}</option>
                    ))}
                  </select>
                </div>
              ))}
            </div>
          </div>

          {/* Runder */}
          <div className="space-y-4">
            <h3 className="font-bold opacity-60 text-sm uppercase tracking-wider">Bracket</h3>
            {tournament.rounds.map(round => (
              <div key={round.id} className="bg-[var(--bg-primary)] rounded-2xl border border-[var(--border)] overflow-hidden">
                <div className={`px-5 py-3 border-b border-[var(--border)] flex items-center justify-between ${round.isFinal ? "bg-[#c23c84]/10" : ""}`}>
                  <div className="flex items-center gap-3">
                    <span className="text-sm font-bold">{round.name}</span>
                    <span className="text-xs bg-[var(--bg-secondary)] px-2 py-0.5 rounded-lg opacity-60">
                      {round.matchFormat ?? tournament.matchFormat}
                    </span>
                  </div>
                  <div className="flex items-center gap-3">
                    {editingRound === round.id ? (
                      <div className="flex items-center gap-2">
                        <input
                          type="datetime-local"
                          value={schedules[round.id] ?? ""}
                          onChange={e => setSchedules(prev => ({ ...prev, [round.id]: e.target.value }))}
                          className="bg-[var(--bg-secondary)] border border-[var(--border)] px-2 py-1 rounded-lg text-xs outline-none focus:border-[#c23c84]"
                        />
                        <button onClick={() => saveRoundSchedule(round.id)}
                          className="text-xs bg-[#c23c84] text-white px-3 py-1 rounded-lg hover:brightness-110 transition">Gem</button>
                        <button onClick={() => setEditingRound(null)}
                          className="text-xs opacity-50 hover:opacity-100 px-2 py-1 transition">✕</button>
                      </div>
                    ) : (
                      <button onClick={() => {
                        setSchedules(prev => ({ ...prev, [round.id]: round.scheduledAt ? round.scheduledAt.slice(0, 16) : "" }));
                        setEditingRound(round.id);
                      }} className="text-xs opacity-50 hover:opacity-100 transition">
                        🕐 {round.scheduledAt ? formatDT(round.scheduledAt) : "Intet tidspunkt"}
                      </button>
                    )}
                  </div>
                </div>
                <div className="divide-y divide-[var(--border)]">
                  {round.matches.map(bm => (
                    <div key={bm.id} className="px-5 py-4">
                      <div className="flex items-center justify-between flex-wrap gap-3">
                        <div className="flex items-center gap-4">
                          <TeamSlot team={bm.team1} isWinner={bm.winnerId === bm.team1?.id} isDone={bm.status === "COMPLETED"} />
                          <span className="text-xs opacity-40">vs</span>
                          <TeamSlot team={bm.team2} isWinner={bm.winnerId === bm.team2?.id} isDone={bm.status === "COMPLETED"} />
                        </div>
                        {bm.status !== "COMPLETED" && bm.team1 && bm.team2 && (
                          <div className="flex gap-2">
                            <button onClick={() => setWinner(bm.id, bm.team1!.id)}
                              className="bg-blue-900/30 hover:bg-blue-900/50 text-blue-400 px-3 py-1.5 rounded-lg text-xs transition">
                              {bm.team1.name} vinder
                            </button>
                            <button onClick={() => setWinner(bm.id, bm.team2!.id)}
                              className="bg-orange-900/30 hover:bg-orange-900/50 text-orange-400 px-3 py-1.5 rounded-lg text-xs transition">
                              {bm.team2.name} vinder
                            </button>
                          </div>
                        )}
                        {bm.match?.id && (
                          <a href={`/kampe/${bm.match.id}`} target="_blank" rel="noreferrer"
                            className="text-xs text-[#e05aa0] hover:underline">Se kamp →</a>
                        )}
                        {bm.status === "PENDING" && (!bm.team1 || !bm.team2) && (
                          <span className="text-xs opacity-40">Afventer tidligere runde</span>
                        )}
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function TeamSlot({ team, isWinner, isDone }: { team: { id: string; name: string; logoUrl: string | null } | null; isWinner: boolean; isDone: boolean }) {
  if (!team) return <span className="text-xs opacity-30 italic">TBD</span>;
  return (
    <div className={`flex items-center gap-2 ${isDone && !isWinner ? "opacity-50" : ""}`}>
      <div className="w-7 h-7 rounded-lg bg-[var(--bg-secondary)] overflow-hidden">
        {team.logoUrl
          ? <img src={`https://api.esportserien.dk${team.logoUrl}`} className="w-full h-full object-cover" alt="" />
          : <div className="w-full h-full flex items-center justify-center text-xs font-bold text-[#e05aa0]">{team.name[0]}</div>
        }
      </div>
      <span className={`text-sm font-semibold ${isWinner ? "text-[#e05aa0]" : ""}`}>{team.name}</span>
      {isWinner && <span className="text-green-400 text-xs">✓</span>}
    </div>
  );
}
```

- [ ] **Step 2: Commit**

```bash
cd /root/esport-web-main
git add app/admin/exhibition/page.tsx
git commit -m "feat(admin): add exhibition tournament admin page"
```

---

## Task 8: Admin Forside — tilføj exhibition link

**Filer:**
- Modificér: `/root/esport-web-main/app/admin/page.tsx`

- [ ] **Step 1: Tilføj exhibition-link i links-arrayet**

Find `const links = [` i `/root/esport-web-main/app/admin/page.tsx`. Tilføj følgende linje i arrayet (fx efter matches-linket):

```typescript
  { href: "/admin/exhibition", title: "Exhibition-turnering", desc: "Opret og administrer standalone bracket-turneringer" },
```

- [ ] **Step 2: Commit**

```bash
cd /root/esport-web-main
git add app/admin/page.tsx
git commit -m "feat(admin): add exhibition tournament link to admin overview"
```

---

## Task 9: Stilling-siden — exhibition detektion

**Filer:**
- Modificér: `/root/esport-web-main/app/stilling/page.tsx`

- [ ] **Step 1: Tilføj exhibitionTournament state**

Find de eksisterende state-deklarationer øverst i `StillingPage`:
```typescript
  const [allTournaments, setAllTournaments] = useState<any[]>([]);
```

Tilføj direkte efter:
```typescript
  const [exhibitionTournament, setExhibitionTournament] = useState<any>(null);
```

- [ ] **Step 2: Sæt exhibitionTournament ved fetch**

Find blokken der sætter `allTournaments` og `view`:
```typescript
        const all = Array.isArray(data) ? data : [];
        setAllTournaments(all);
        setLoading(false);
        // Vis mesterskabsspillet som default hvis det findes
        const hasChampionship = all.some((t: any) => t.type === "CHAMPIONSHIP" || t.type === "PROMOTION");
        setView(hasChampionship ? "playoffs" : "table");
```

Erstat med:
```typescript
        const all = Array.isArray(data) ? data : [];
        setAllTournaments(all);
        setLoading(false);
        const exhibition = all.find((t: any) => t.type === "EXHIBITION") ?? null;
        setExhibitionTournament(exhibition);
        if (exhibition) {
          setView("exhibition");
        } else {
          const hasChampionship = all.some((t: any) => t.type === "CHAMPIONSHIP" || t.type === "PROMOTION");
          setView(hasChampionship ? "playoffs" : "table");
        }
```

- [ ] **Step 3: Spring liga-standings over ved exhibition**

Find `Promise.all([` blokken der beregner `leagueStandings`. Wrap hele blokken med en guard:

```typescript
    // Beregn kun liga-stilling når sæsonen ikke er exhibition
    if (!exhibition) {
      Promise.all([
        fetch(`https://api.esportserien.dk/seasons/${seasonId}/teams`).then(r => r.json()),
        fetch(`https://api.esportserien.dk/seasons/matches?seasonId=${seasonId}`).then(r => r.json())
      ]).then(([teams, matches]) => {
        // ...hele den eksisterende standings-beregning uændret...
      });
    }
```

**OBS:** `exhibition`-variablen er lokal til `.then`-callbacket. Brug `exhibitionTournament` state hvis du er udenfor scopet, men i dette tilfælde er `exhibition` tilgængelig fordi begge blokke er i samme `useEffect`-callback.

- [ ] **Step 4: Skjul tab-knapper og liga-tabel ved exhibition, tilføj exhibition bracket**

Find tab-knapperne i JSX:
```typescript
          <div className="flex flex-wrap bg-[var(--bg-primary)] p-1 rounded-xl border border-[var(--border)] gap-1">
            <button onClick={() => setView("table")}
```

Wrap hele `<div className="flex flex-wrap ...">...</div>` blokken med:
```tsx
          {!exhibitionTournament && (
            <div className="flex flex-wrap bg-[var(--bg-primary)] p-1 rounded-xl border border-[var(--border)] gap-1">
              <button onClick={() => setView("table")}
                className={`px-3 py-1.5 rounded-lg text-sm font-semibold transition ${view === "table" ? "bg-[#c23c84] text-[var(--text-primary)]" : "opacity-60 hover:opacity-70"}`}>
                Liga
              </button>
              {playoffsTournament && (
                <button onClick={() => setView("playoffs")}
                  className={`px-3 py-1.5 rounded-lg text-sm font-semibold transition ${view === "playoffs" ? "bg-[#c23c84] text-[var(--text-primary)]" : "opacity-60 hover:opacity-70"}`}>
                  {playoffsTournament.type === "CHAMPIONSHIP" ? "Mesterskab" : "Oprykning"}
                </button>
              )}
              {relegationTournament && (
                <button onClick={() => setView("relegation")}
                  className={`px-3 py-1.5 rounded-lg text-sm font-semibold transition ${view === "relegation" ? "bg-orange-600 text-[var(--text-primary)]" : "opacity-60 hover:opacity-70"}`}>
                  Nedrykning
                </button>
              )}
            </div>
          )}
```

Tilføj derefter exhibition-visning i render-sektionen (fx efter `{/* LIGA TABEL */}` blokken):

```tsx
      {/* EXHIBITION BRACKET */}
      {view === "exhibition" && exhibitionTournament && (
        <div className="space-y-4">
          {exhibitionTournament.status === "FINISHED" && (() => {
            const finalRound = exhibitionTournament.rounds?.find((r: any) => r.isFinal);
            const winner = finalRound?.matches?.[0]?.winner;
            if (!winner) return null;
            return (
              <div className="bg-gradient-to-r from-yellow-900/30 to-[#c23c84]/20 border border-yellow-700/40 rounded-2xl p-5 flex items-center gap-4">
                <div>
                  <div className="text-xs opacity-50 uppercase tracking-wider">Turneringsvinder</div>
                  <div className="font-extrabold text-xl text-yellow-400">{winner.name}</div>
                </div>
                {winner.logoUrl && (
                  <div className="ml-auto w-12 h-12 rounded-xl overflow-hidden">
                    <img src={`https://api.esportserien.dk${winner.logoUrl}`} className="w-full h-full object-cover" />
                  </div>
                )}
              </div>
            );
          })()}
          <div className="bg-[#c23c84]/10 border border-[#c23c84]/30 rounded-2xl px-5 py-3">
            <span className="text-sm font-bold text-[#e05aa0]">{exhibitionTournament.name}</span>
            <span className="text-xs opacity-60 ml-2">{exhibitionTournament.seeds?.length} hold · Single Elimination</span>
          </div>
          <BracketView tournament={exhibitionTournament} />
        </div>
      )}

      {/* LIGA TABEL */}
      {!exhibitionTournament && view === "table" && (
        <LeagueTable standings={leagueStandings} splitAt={split} hasPlayoffs={!!playoffsTournament} />
      )}
```

**OBS:** Fjern det eksisterende `{/* LIGA TABEL */}` render-kald og erstat med ovenstående (som tilføjer `!exhibitionTournament` guard).

- [ ] **Step 5: Commit**

```bash
cd /root/esport-web-main
git add app/stilling/page.tsx
git commit -m "feat(stilling): detect exhibition tournament and show bracket instead of liga table"
```

---

## Task 10: Frontend Deploy + End-to-End Verificering

- [ ] **Step 1: Deploy frontend**

```bash
ssh -i ~/.ssh/id_ed25519 root@92.118.207.29
cd /root/esport-prod
docker compose build frontend && docker compose up -d frontend
```

Vent ~60 sekunder (Next.js build). Verificér:
```bash
docker logs esport-prod-frontend-1 --tail 20
```

Forventet: ingen build-fejl.

- [ ] **Step 2: End-to-end verificering**

1. Gå til `https://esportserien.dk/admin/` → "Exhibition-turnering" link er synligt
2. Gå til `https://esportserien.dk/admin/exhibition/` → vælg en sæson med hold → klik "Opret turnering"
3. Trin 1: angiv navn + template → klik "Næste: Seeding"
4. Trin 2: klik "Auto-seed via FACEIT ELO" → hold sorteres med ELO → klik "Opret og generer bracket"
5. Trin 3: angiv tidspunkter for QF, SF, F → klik "Gem tidsplan"
6. Verificér at bracket-admin vises med korrekte runde-navne og matchformater (BO1/BO3)
7. Klik "Generer kampe" → bekræft at kampe oprettes (grøn besked: "X kampe genereret!")
8. Gå til `https://esportserien.dk/stilling/` → vælg sæsonen → verificér at kun bracket vises (ingen Liga-tab)
9. Sæt en vinder på en QF-kamp → verificér at vinder rykker videre til SF og en ny SF-kamp oprettes automatisk med BO1 format
10. Sæt begge SF-vindere → verificér at finalen oprettes automatisk med BO3 format
