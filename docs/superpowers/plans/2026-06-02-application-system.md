# Application System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tilføj et dynamisk ansøgningssystem til esportserien.dk så hold-captains kan ansøge til sæsoner via admin-byggede multi-step polls.

**Architecture:** NestJS backend modul (`ApplicationsModule`) med Prisma-modeller, captain-endpoints og admin-endpoints. Next.js frontend med ansøgningsformular (`/hold/[slug]/ansoeg`) og admin-side (`/admin/ansoegninger`). Hold-siden får ny knap ved siden af "Administrer hold".

**Tech Stack:** NestJS + Prisma + PostgreSQL (backend), Next.js 14 App Router + Tailwind CSS (frontend), `@nestjs-modules/mailer` til email.

**Server info:**
- Backend kode: `root@92.118.207.29:/root/esports-platform-main/api/`
- Frontend kode: `root@92.118.207.29:/root/esport-web-main/`
- Deploy: `cd /root/esport-prod && docker compose build <service> && docker compose up -d <service>`
- SSH: `ssh -i ~/.ssh/id_ed25519 root@92.118.207.29`

---

## Task 1: Prisma Schema Migration

**Files:**
- Modify: `/root/esports-platform-main/api/prisma/schema.prisma`

- [ ] **Step 1: Tilføj back-references til eksisterende modeller**

I `schema.prisma`, find `model Season` og tilføj én linje i bunden af modellen (før den afsluttende `}`):
```prisma
  applicationPolls ApplicationPoll[]
```

Find `model Team` og tilføj:
```prisma
  applications     ApplicationSubmission[]
```

Find `model User` og tilføj:
```prisma
  captainApplications ApplicationSubmission[] @relation("CaptainApplications")
```

- [ ] **Step 2: Tilføj nye enums og modeller**

Tilføj følgende blok i bunden af `schema.prisma` (efter de eksisterende enums):

```prisma
enum PollStatus {
  DRAFT
  OPEN
  CLOSED
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

model ApplicationPoll {
  id          String   @id @default(uuid())
  title       String
  description String?
  seasonId    String?
  status      PollStatus @default(DRAFT)
  steps       Json
  createdAt   DateTime @default(now())
  updatedAt   DateTime @updatedAt

  season      Season?  @relation(fields: [seasonId], references: [id])
  submissions ApplicationSubmission[]
}

model ApplicationSubmission {
  id            String           @id @default(uuid())
  pollId        String
  teamId        String
  captainId     String
  applicantType ApplicantType
  status        SubmissionStatus @default(PENDING)
  answers       Json
  createdAt     DateTime         @default(now())
  updatedAt     DateTime         @updatedAt

  poll    ApplicationPoll @relation(fields: [pollId], references: [id])
  team    Team            @relation(fields: [teamId], references: [id])
  captain User            @relation("CaptainApplications", fields: [captainId], references: [id])

  @@unique([pollId, teamId])
}
```

- [ ] **Step 3: Kør migration**

SSH til serveren og kør:
```bash
ssh -i ~/.ssh/id_ed25519 root@92.118.207.29
cd /root/esports-platform-main/api
DATABASE_URL="postgresql://esport:supersecret@localhost:5432/esportdb" npx prisma migrate dev --name add_application_system
```

Forventet output: `The following migration(s) have been applied: ...add_application_system`

Hvis `localhost:5432` ikke virker (postgres er i Docker), brug:
```bash
DATABASE_URL="postgresql://esport:supersecret@$(docker inspect esport-prod-postgres-1 --format '{{.NetworkSettings.Networks.esport_prod_default.IPAddress}}'):5432/esportdb" npx prisma migrate dev --name add_application_system
```

- [ ] **Step 4: Verificér tabeller**

```bash
docker exec esport-prod-postgres-1 psql -U esport -d esportdb -c "\dt" | grep -i application
```

Forventet output:
```
 public | ApplicationPoll       | table | esport
 public | ApplicationSubmission | table | esport
```

- [ ] **Step 5: Commit**

```bash
cd /root/esports-platform-main
git add api/prisma/schema.prisma api/prisma/migrations/
git commit -m "feat(db): add ApplicationPoll and ApplicationSubmission models"
```

---

## Task 2: ApplicationsService

**Files:**
- Create: `/root/esports-platform-main/api/src/applications/applications.service.ts`

- [ ] **Step 1: Opret filen med fuld implementation**

```bash
mkdir -p /root/esports-platform-main/api/src/applications
```

Opret `/root/esports-platform-main/api/src/applications/applications.service.ts`:

```typescript
import {
  Injectable,
  NotFoundException,
  ForbiddenException,
  BadRequestException,
  ConflictException,
} from '@nestjs/common';
import { PrismaService } from '../prisma/prisma.service';
import { MailerService } from '@nestjs-modules/mailer';

@Injectable()
export class ApplicationsService {
  constructor(
    private prisma: PrismaService,
    private mailerService: MailerService,
  ) {}

  async getActivePoll() {
    const poll = await this.prisma.applicationPoll.findFirst({
      where: { status: 'OPEN' },
      orderBy: { createdAt: 'desc' },
      include: { season: { select: { name: true } } },
    });
    if (!poll) throw new NotFoundException('Ingen aktiv ansøgningspoll');
    return poll;
  }

  async getMySubmission(teamId: string) {
    const poll = await this.prisma.applicationPoll.findFirst({
      where: { status: 'OPEN' },
      orderBy: { createdAt: 'desc' },
    });
    if (!poll) return { hasApplied: false, status: null };

    const submission = await this.prisma.applicationSubmission.findUnique({
      where: { pollId_teamId: { pollId: poll.id, teamId } },
    });
    return {
      hasApplied: !!submission,
      status: submission?.status ?? null,
      submissionId: submission?.id ?? null,
    };
  }

  async submitApplication(
    pollId: string,
    captainId: string,
    body: { teamId: string; applicantType: string; answers: Record<string, any> },
  ) {
    const team = await this.prisma.team.findUnique({ where: { id: body.teamId } });
    if (!team) throw new NotFoundException('Hold ikke fundet');
    if (team.captainId !== captainId)
      throw new ForbiddenException('Kun captains kan ansøge');

    const poll = await this.prisma.applicationPoll.findUnique({ where: { id: pollId } });
    if (!poll) throw new NotFoundException('Poll ikke fundet');
    if (poll.status !== 'OPEN')
      throw new BadRequestException('Ansøgninger er ikke åbne');

    const steps = poll.steps as any[];
    for (const step of steps) {
      for (const q of step.questions) {
        if (q.required) {
          const answer = body.answers[q.id];
          const isEmpty =
            !answer ||
            (Array.isArray(answer) && answer.length === 0) ||
            answer === '';
          if (isEmpty)
            throw new BadRequestException(`Spørgsmål "${q.label}" er påkrævet`);
        }
      }
    }

    try {
      return await this.prisma.applicationSubmission.create({
        data: {
          pollId,
          teamId: body.teamId,
          captainId,
          applicantType: body.applicantType as any,
          answers: body.answers,
        },
      });
    } catch (e: any) {
      if (e.code === 'P2002') throw new ConflictException('Holdet har allerede ansøgt');
      throw e;
    }
  }

  async getAllPolls() {
    return this.prisma.applicationPoll.findMany({
      orderBy: { createdAt: 'desc' },
      include: {
        season: { select: { name: true } },
        _count: { select: { submissions: true } },
      },
    });
  }

  async createPoll(data: {
    title: string;
    description?: string;
    seasonId?: string;
    steps: any[];
    status?: string;
  }) {
    return this.prisma.applicationPoll.create({
      data: {
        title: data.title,
        description: data.description,
        seasonId: data.seasonId || null,
        steps: data.steps,
        status: (data.status as any) ?? 'DRAFT',
      },
    });
  }

  async updatePoll(
    id: string,
    data: {
      title?: string;
      description?: string;
      seasonId?: string;
      steps?: any[];
      status?: string;
    },
  ) {
    return this.prisma.applicationPoll.update({
      where: { id },
      data: {
        ...(data.title !== undefined && { title: data.title }),
        ...(data.description !== undefined && { description: data.description }),
        ...(data.seasonId !== undefined && { seasonId: data.seasonId || null }),
        ...(data.steps !== undefined && { steps: data.steps }),
        ...(data.status !== undefined && { status: data.status as any }),
      },
    });
  }

  async deletePoll(id: string) {
    const poll = await this.prisma.applicationPoll.findUnique({
      where: { id },
      include: { _count: { select: { submissions: true } } },
    });
    if (!poll) throw new NotFoundException('Poll ikke fundet');
    if (poll.status !== 'DRAFT')
      throw new ForbiddenException('Kun DRAFT polls kan slettes');
    if (poll._count.submissions > 0)
      throw new ForbiddenException('Polls med ansøgninger kan ikke slettes');
    return this.prisma.applicationPoll.delete({ where: { id } });
  }

  async getPollSubmissions(pollId: string) {
    return this.prisma.applicationSubmission.findMany({
      where: { pollId },
      orderBy: { createdAt: 'desc' },
      include: {
        team: { select: { name: true, slug: true, logoUrl: true } },
        captain: { select: { username: true, email: true, firstName: true, lastName: true } },
      },
    });
  }

  async updateSubmissionStatus(pollId: string, submissionId: string, status: string) {
    const submission = await this.prisma.applicationSubmission.findUnique({
      where: { id: submissionId },
      include: {
        team: true,
        captain: true,
        poll: { include: { season: true } },
      },
    });
    if (!submission) throw new NotFoundException('Ansøgning ikke fundet');
    if (submission.pollId !== pollId)
      throw new ForbiddenException('Ansøgning tilhører ikke denne poll');

    const updated = await this.prisma.applicationSubmission.update({
      where: { id: submissionId },
      data: { status: status as any },
    });

    if (status === 'CONFIRMED') {
      const seasonName = submission.poll.season?.name ?? submission.poll.title;
      const typeLabel =
        { TEAM: 'hold', ORG: 'organisation', SCHOOL: 'efterskole/institution' }[
          submission.applicantType
        ] ?? '';

      await this.mailerService.sendMail({
        from: process.env.EMAIL_FROM,
        to: submission.captain.email,
        subject: `Jeres ansøgning til ${seasonName} er bekræftet — Esportserien`,
        html: `<!DOCTYPE html><html lang="da"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0"></head><body style="margin:0;padding:0;background:#1a1220;font-family:Arial,sans-serif;"><table width="100%" cellpadding="0" cellspacing="0" bgcolor="#120d18" style="background-color:#120d18;padding:40px 20px;"><tr><td align="center"><table width="600" cellpadding="0" cellspacing="0"><tr><td align="center" style="padding-bottom:30px;"><table cellpadding="0" cellspacing="0"><tr><td style="background-color:#c23c84;border-radius:16px;padding:16px 24px;"><span style="color:#fff;font-size:24px;font-weight:900;letter-spacing:2px;">ESPORTSERIEN</span></td></tr></table></td></tr><tr><td bgcolor="#1a1228" style="background-color:#1a1228;border-radius:24px;padding:48px 40px;border:2px solid #3d1f4d;"><table width="100%" cellpadding="0" cellspacing="0"><tr><td align="center" style="padding-bottom:24px;font-size:48px;">🎉</td></tr><tr><td align="center" style="padding-bottom:12px;"><h1 style="margin:0;color:#ffffff;font-size:28px;font-weight:900;">Ansøgning bekræftet!</h1></td></tr><tr><td align="center" style="padding-bottom:32px;"><p style="margin:0;color:#c8b8d0;font-size:16px;line-height:1.6;">Tillykke, <strong style="color:#fff;">${submission.team.name}</strong>!<br><br>Jeres ansøgning som <strong style="color:#e05aa0;">${typeLabel}</strong> til <strong style="color:#fff;">${seasonName}</strong> er bekræftet. Vi kontakter jer snart med næste skridt.</p></td></tr><tr><td align="center" style="padding-bottom:32px;"><a href="${process.env.FRONTEND_URL}/hold/${submission.team.slug}" style="display:inline-block;background-color:#c23c84;color:#ffffff;text-decoration:none;font-size:16px;font-weight:700;padding:16px 48px;border-radius:12px;border:2px solid #ff5ca8;">Se jeres holdside</a></td></tr></table></td></tr><tr><td align="center" style="padding-top:24px;"><p style="margin:0;color:#4a3f4f;font-size:12px;">© 2026 Esportserien I/S · CVR: 46278267 · <a href="https://esportserien.dk" style="color:#c23c84;text-decoration:none;">esportserien.dk</a></p></td></tr></table></td></tr></table></body></html>`,
      });
    }

    return updated;
  }
}
```

- [ ] **Step 2: Commit**

```bash
cd /root/esports-platform-main
git add api/src/applications/applications.service.ts
git commit -m "feat(applications): add ApplicationsService"
```

---

## Task 3: Captain Controller

**Files:**
- Create: `/root/esports-platform-main/api/src/applications/applications.controller.ts`

- [ ] **Step 1: Opret controller**

Opret `/root/esports-platform-main/api/src/applications/applications.controller.ts`:

```typescript
import { Controller, Get, Post, Param, Body, Req, UseGuards } from '@nestjs/common';
import { ApplicationsService } from './applications.service';
import { JwtAuthGuard } from '../auth/jwt.guard';

@Controller('applications')
export class ApplicationsController {
  constructor(private applicationsService: ApplicationsService) {}

  @Get('active')
  getActivePoll() {
    return this.applicationsService.getActivePoll();
  }

  @UseGuards(JwtAuthGuard)
  @Get('my/:teamId')
  getMySubmission(@Param('teamId') teamId: string) {
    return this.applicationsService.getMySubmission(teamId);
  }

  @UseGuards(JwtAuthGuard)
  @Post(':pollId/submit')
  submitApplication(
    @Param('pollId') pollId: string,
    @Body() body: { teamId: string; applicantType: string; answers: Record<string, any> },
    @Req() req,
  ) {
    return this.applicationsService.submitApplication(pollId, req.user.sub, body);
  }
}
```

- [ ] **Step 2: Commit**

```bash
cd /root/esports-platform-main
git add api/src/applications/applications.controller.ts
git commit -m "feat(applications): add captain endpoints controller"
```

---

## Task 4: Admin Controller

**Files:**
- Create: `/root/esports-platform-main/api/src/applications/admin-applications.controller.ts`

- [ ] **Step 1: Opret admin controller**

Opret `/root/esports-platform-main/api/src/applications/admin-applications.controller.ts`:

```typescript
import {
  Controller,
  Get,
  Post,
  Put,
  Delete,
  Patch,
  Param,
  Body,
  UseGuards,
} from '@nestjs/common';
import { ApplicationsService } from './applications.service';
import { JwtAuthGuard } from '../auth/jwt.guard';
import { RoleGuard } from '../auth/role.guard';

@UseGuards(JwtAuthGuard, new RoleGuard('ADMIN'))
@Controller('admin/applications')
export class AdminApplicationsController {
  constructor(private applicationsService: ApplicationsService) {}

  @Get()
  getAllPolls() {
    return this.applicationsService.getAllPolls();
  }

  @Post()
  createPoll(
    @Body()
    body: {
      title: string;
      description?: string;
      seasonId?: string;
      steps: any[];
      status?: string;
    },
  ) {
    return this.applicationsService.createPoll(body);
  }

  @Put(':id')
  updatePoll(
    @Param('id') id: string,
    @Body()
    body: {
      title?: string;
      description?: string;
      seasonId?: string;
      steps?: any[];
      status?: string;
    },
  ) {
    return this.applicationsService.updatePoll(id, body);
  }

  @Delete(':id')
  deletePoll(@Param('id') id: string) {
    return this.applicationsService.deletePoll(id);
  }

  @Get(':id/submissions')
  getPollSubmissions(@Param('id') id: string) {
    return this.applicationsService.getPollSubmissions(id);
  }

  @Patch(':id/submissions/:sid')
  updateSubmissionStatus(
    @Param('id') id: string,
    @Param('sid') sid: string,
    @Body() body: { status: string },
  ) {
    return this.applicationsService.updateSubmissionStatus(id, sid, body.status);
  }
}
```

- [ ] **Step 2: Commit**

```bash
cd /root/esports-platform-main
git add api/src/applications/admin-applications.controller.ts
git commit -m "feat(applications): add admin endpoints controller"
```

---

## Task 5: Module Registration + Backend Deploy

**Files:**
- Create: `/root/esports-platform-main/api/src/applications/applications.module.ts`
- Modify: `/root/esports-platform-main/api/src/app.module.ts`

- [ ] **Step 1: Opret module**

Opret `/root/esports-platform-main/api/src/applications/applications.module.ts`:

```typescript
import { Module } from '@nestjs/common';
import { ApplicationsService } from './applications.service';
import { ApplicationsController } from './applications.controller';
import { AdminApplicationsController } from './admin-applications.controller';
import { PrismaModule } from '../prisma/prisma.module';

@Module({
  imports: [PrismaModule],
  providers: [ApplicationsService],
  controllers: [ApplicationsController, AdminApplicationsController],
})
export class ApplicationsModule {}
```

- [ ] **Step 2: Registrer i app.module.ts**

I `/root/esports-platform-main/api/src/app.module.ts`:

Tilføj import øverst:
```typescript
import { ApplicationsModule } from './applications/applications.module';
```

Tilføj `ApplicationsModule` i `imports`-arrayet (efter `DivisionsModule`):
```typescript
    DivisionsModule,
    ApplicationsModule,
```

- [ ] **Step 3: Build og verificér**

```bash
cd /root/esports-platform-main/api
npm run build 2>&1 | tail -20
```

Forventet output: `Successfully compiled` eller `webpack compiled successfully`. Ingen TypeScript-fejl.

- [ ] **Step 4: Deploy backend**

```bash
cd /root/esport-prod
docker compose build backend && docker compose up -d backend
```

Vent ~30 sekunder, verificér at containeren kører:
```bash
docker ps | grep backend
curl -s http://localhost:3000/applications/active | head -c 100
```

Forventet: enten `{"statusCode":404,"message":"Ingen aktiv ansøgningspoll"}` eller en poll-JSON.

- [ ] **Step 5: Commit**

```bash
cd /root/esports-platform-main
git add api/src/applications/applications.module.ts api/src/app.module.ts
git commit -m "feat(applications): register ApplicationsModule in app"
```

---

## Task 6: Hold-siden — ny ansøgningsknap

**Files:**
- Modify: `/root/esport-web-main/app/hold/[slug]/page.tsx`

- [ ] **Step 1: Tilføj state og imports**

Find øverst i `page.tsx` (hold/[slug]):
```typescript
const [userId, setUserId] = useState<string | null>(null);
```

Tilføj to nye state-variabler direkte efter denne linje:
```typescript
  const [activePoll, setActivePoll] = useState<{ id: string; title: string } | null>(null);
  const [mySubmission, setMySubmission] = useState<{ hasApplied: boolean; status: string | null } | null>(null);
```

- [ ] **Step 2: Tilføj effect til at hente poll og submission**

Find den eksisterende `isCaptainUser` beregning:
```typescript
  const isCaptainUser = userId && team && userId === team.captainId;
```

Tilføj følgende useEffect direkte efter denne linje:
```typescript
  useEffect(() => {
    if (!isCaptainUser || !team) return;
    fetch("https://api.esportserien.dk/applications/active")
      .then(r => r.ok ? r.json() : null)
      .then(setActivePoll)
      .catch(() => {});
    const token = localStorage.getItem("token");
    fetch(`https://api.esportserien.dk/applications/my/${team.id}`, {
      headers: token ? { Authorization: `Bearer ${token}` } : {},
    })
      .then(r => r.json())
      .then(setMySubmission)
      .catch(() => {});
  }, [isCaptainUser, team?.id]);
```

- [ ] **Step 3: Tilføj knap i UI**

Find denne blok i JSX:
```typescript
          {userId === team.captainId && (
            <Link href={`/hold/${team.slug}/admin`} className="text-xs bg-[#c23c84]/20 text-[#e05aa0] px-3 py-1.5 rounded-lg hover:bg-[#c23c84]/30 transition font-semibold">
              Administrer hold
            </Link>
          )}
```

Erstat med:
```typescript
          {userId === team.captainId && (
            <Link href={`/hold/${team.slug}/admin`} className="text-xs bg-[#c23c84]/20 text-[#e05aa0] px-3 py-1.5 rounded-lg hover:bg-[#c23c84]/30 transition font-semibold">
              Administrer hold
            </Link>
          )}
          {isCaptainUser && activePoll && (
            mySubmission?.hasApplied
              ? <Link href={`/hold/${team.slug}/ansoeg`} className="text-xs bg-green-900/40 text-green-400 border border-green-800/40 px-3 py-1.5 rounded-lg font-semibold hover:bg-green-900/60 transition">
                  Ansøgning indsendt ✓
                </Link>
              : <Link href={`/hold/${team.slug}/ansoeg`} className="text-xs bg-[var(--bg-secondary)] border border-[var(--border)] px-3 py-1.5 rounded-lg hover:border-[#c23c84]/50 transition font-semibold">
                  Ansøg til sæson →
                </Link>
          )}
```

- [ ] **Step 4: Commit**

```bash
cd /root/esport-web-main
git add "app/hold/[slug]/page.tsx"
git commit -m "feat(hold): add season application button for captains"
```

---

## Task 7: Ansøgningsformular

**Files:**
- Create: `/root/esport-web-main/app/hold/[slug]/ansoeg/page.tsx`

- [ ] **Step 1: Opret mappe og fil**

```bash
mkdir -p /root/esport-web-main/app/hold/\[slug\]/ansoeg
```

Opret `/root/esport-web-main/app/hold/[slug]/ansoeg/page.tsx`:

```tsx
"use client";

import { useEffect, useState } from "react";
import { useParams, useRouter } from "next/navigation";
import Link from "next/link";

export default function AnsoegPage() {
  const { slug } = useParams();
  const router = useRouter();

  const [poll, setPoll] = useState<any>(null);
  const [team, setTeam] = useState<any>(null);
  const [step, setStep] = useState(0);
  const [applicantType, setApplicantType] = useState("");
  const [answers, setAnswers] = useState<Record<string, any>>({});
  const [loading, setLoading] = useState(false);
  const [submitted, setSubmitted] = useState(false);
  const [error, setError] = useState("");
  const [validationError, setValidationError] = useState("");

  useEffect(() => {
    const token = localStorage.getItem("token");
    if (!token) { router.push("/login"); return; }

    fetch(`https://api.esportserien.dk/teams/slug/${slug}`)
      .then(r => r.json())
      .then(t => {
        try {
          const payload = JSON.parse(atob(token.split(".")[1]));
          if (t.captainId !== payload.sub) { router.push(`/hold/${slug}`); return; }
        } catch { router.push(`/hold/${slug}`); return; }
        setTeam(t);
      })
      .catch(() => router.push(`/hold/${slug}`));

    fetch("https://api.esportserien.dk/applications/active")
      .then(r => { if (!r.ok) { router.push(`/hold/${slug}`); return null; } return r.json(); })
      .then(p => { if (p) setPoll(p); })
      .catch(() => router.push(`/hold/${slug}`));
  }, [slug]);

  if (!poll || !team) {
    return <div className="text-center py-20 animate-pulse opacity-60">Indlæser...</div>;
  }

  if (submitted) {
    return (
      <div className="max-w-lg mx-auto text-center py-20 space-y-4">
        <div className="text-5xl">🎉</div>
        <h1 className="text-2xl font-bold">Ansøgning indsendt!</h1>
        <p className="opacity-60">Vi gennemgår jeres ansøgning og vender tilbage hurtigst muligt.</p>
        <Link href={`/hold/${slug}`} className="inline-block bg-[#c23c84] text-white px-6 py-2.5 rounded-xl font-semibold hover:bg-[#a02870] transition">
          Tilbage til holdsiden
        </Link>
      </div>
    );
  }

  const pollSteps = (poll.steps ?? []) as any[];
  const totalSteps = 1 + pollSteps.length + 1;
  const progress = Math.round((step / Math.max(totalSteps - 1, 1)) * 100);

  const TYPE_LABELS: Record<string, string> = {
    TEAM: "Hold",
    ORG: "Organisation",
    SCHOOL: "Efterskole/Institution",
  };

  const setAnswer = (qId: string, value: any) =>
    setAnswers(prev => ({ ...prev, [qId]: value }));

  const toggleCheckbox = (qId: string, option: string) => {
    const current = (answers[qId] as string[]) ?? [];
    setAnswers(prev => ({
      ...prev,
      [qId]: current.includes(option)
        ? current.filter(o => o !== option)
        : [...current, option],
    }));
  };

  const validateStep = (): boolean => {
    if (step === 0) {
      if (!applicantType) { setValidationError("Vælg en type for at fortsætte"); return false; }
      return true;
    }
    const pollStep = pollSteps[step - 1];
    if (!pollStep) return true;
    for (const q of pollStep.questions) {
      if (q.required) {
        const ans = answers[q.id];
        const empty = !ans || (Array.isArray(ans) && ans.length === 0) || ans === "";
        if (empty) { setValidationError(`"${q.label}" er påkrævet`); return false; }
      }
    }
    return true;
  };

  const next = () => {
    setValidationError("");
    if (!validateStep()) return;
    setStep(s => s + 1);
  };

  const back = () => { setValidationError(""); setStep(s => s - 1); };

  const submit = async () => {
    setLoading(true);
    setError("");
    const token = localStorage.getItem("token");
    try {
      const res = await fetch(
        `https://api.esportserien.dk/applications/${poll.id}/submit`,
        {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            Authorization: `Bearer ${token}`,
          },
          body: JSON.stringify({ teamId: team.id, applicantType, answers }),
        },
      );
      if (res.ok) {
        setSubmitted(true);
      } else {
        const data = await res.json().catch(() => ({}));
        setError(data.message ?? "Noget gik galt, prøv igen");
      }
    } catch {
      setError("Netværksfejl, prøv igen");
    }
    setLoading(false);
  };

  return (
    <div className="max-w-2xl mx-auto space-y-6">
      <div>
        <h1 className="text-2xl font-bold">Ansøg til sæson</h1>
        <p className="text-sm opacity-50 mt-1">
          {team.name} · {poll.title}
        </p>
      </div>

      {/* Progress bar */}
      <div className="space-y-2">
        <div className="flex justify-between text-xs opacity-50">
          <span>Trin {step + 1} af {totalSteps}</span>
          <span>{progress}%</span>
        </div>
        <div className="h-1.5 bg-[var(--bg-secondary)] rounded-full overflow-hidden">
          <div
            className="h-full bg-[#c23c84] transition-all duration-300 ease-out"
            style={{ width: `${progress}%` }}
          />
        </div>
      </div>

      <div className="bg-[var(--bg-secondary)] rounded-2xl p-6 space-y-6">

        {/* Step 0: Ansøgertype */}
        {step === 0 && (
          <div className="space-y-4">
            <h2 className="font-bold text-lg">Hvad repræsenterer I?</h2>
            <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
              {(["TEAM", "ORG", "SCHOOL"] as const).map(t => (
                <button
                  key={t}
                  onClick={() => setApplicantType(t)}
                  className={`p-4 rounded-xl border-2 text-sm font-semibold transition text-center ${
                    applicantType === t
                      ? "border-[#c23c84] bg-[#c23c84]/20 text-[#e05aa0]"
                      : "border-[var(--border)] hover:border-[#c23c84]/40"
                  }`}
                >
                  {t === "TEAM" ? "🎮 Hold" : t === "ORG" ? "🏢 Organisation" : "🏫 Efterskole / Institution"}
                </button>
              ))}
            </div>
          </div>
        )}

        {/* Step 1..N: Poll-spørgsmål */}
        {step >= 1 && step <= pollSteps.length && (() => {
          const pollStep = pollSteps[step - 1];
          return (
            <div className="space-y-5">
              <h2 className="font-bold text-lg">{pollStep.title}</h2>
              {pollStep.questions.map((q: any) => (
                <div key={q.id} className="space-y-2">
                  <label className="text-sm font-medium block">
                    {q.label}
                    {q.required && <span className="text-[#c23c84] ml-1">*</span>}
                  </label>
                  {q.type === "short_text" && (
                    <input
                      type="text"
                      value={answers[q.id] ?? ""}
                      onChange={e => setAnswer(q.id, e.target.value)}
                      className="w-full bg-[var(--bg-primary)] border border-[var(--border)] rounded-xl px-4 py-2.5 text-sm focus:border-[#c23c84] focus:outline-none transition"
                    />
                  )}
                  {q.type === "long_text" && (
                    <textarea
                      value={answers[q.id] ?? ""}
                      onChange={e => setAnswer(q.id, e.target.value)}
                      rows={4}
                      className="w-full bg-[var(--bg-primary)] border border-[var(--border)] rounded-xl px-4 py-2.5 text-sm focus:border-[#c23c84] focus:outline-none transition resize-none"
                    />
                  )}
                  {q.type === "radio" && (
                    <div className="space-y-2">
                      {(q.options ?? []).map((opt: string) => (
                        <label key={opt} className="flex items-center gap-3 cursor-pointer">
                          <input
                            type="radio"
                            name={q.id}
                            value={opt}
                            checked={answers[q.id] === opt}
                            onChange={() => setAnswer(q.id, opt)}
                            className="accent-[#c23c84]"
                          />
                          <span className="text-sm">{opt}</span>
                        </label>
                      ))}
                    </div>
                  )}
                  {q.type === "checkbox" && (
                    <div className="space-y-2">
                      {(q.options ?? []).map((opt: string) => (
                        <label key={opt} className="flex items-center gap-3 cursor-pointer">
                          <input
                            type="checkbox"
                            checked={((answers[q.id] as string[]) ?? []).includes(opt)}
                            onChange={() => toggleCheckbox(q.id, opt)}
                            className="accent-[#c23c84]"
                          />
                          <span className="text-sm">{opt}</span>
                        </label>
                      ))}
                    </div>
                  )}
                </div>
              ))}
            </div>
          );
        })()}

        {/* Sidste trin: Gennemse */}
        {step === totalSteps - 1 && (
          <div className="space-y-5">
            <h2 className="font-bold text-lg">Gennemse og indsend</h2>
            <div className="bg-[var(--bg-primary)] rounded-xl p-4 space-y-1">
              <div className="text-xs opacity-50 uppercase tracking-wide">Ansøgertype</div>
              <div className="font-semibold">{TYPE_LABELS[applicantType]}</div>
            </div>
            {pollSteps.map((s: any) => (
              <div key={s.id} className="space-y-2">
                <div className="text-xs font-semibold opacity-50 uppercase tracking-wide">{s.title}</div>
                {s.questions.map((q: any) => {
                  const ans = answers[q.id];
                  if (!ans || (Array.isArray(ans) && ans.length === 0)) return null;
                  return (
                    <div key={q.id} className="bg-[var(--bg-primary)] rounded-xl px-4 py-3 space-y-1">
                      <div className="text-xs opacity-50">{q.label}</div>
                      <div className="text-sm">{Array.isArray(ans) ? ans.join(", ") : ans}</div>
                    </div>
                  );
                })}
              </div>
            ))}
            {error && (
              <div className="text-red-400 text-sm bg-red-900/20 border border-red-800/40 rounded-xl p-3">
                {error}
              </div>
            )}
          </div>
        )}

        {validationError && (
          <div className="text-red-400 text-sm bg-red-900/20 border border-red-800/40 rounded-xl p-3">
            {validationError}
          </div>
        )}
      </div>

      {/* Navigation */}
      <div className="flex justify-between">
        <button
          onClick={back}
          disabled={step === 0}
          className="px-5 py-2.5 rounded-xl border border-[var(--border)] text-sm font-semibold disabled:opacity-30 hover:bg-[var(--bg-secondary)] transition"
        >
          ← Tilbage
        </button>
        {step < totalSteps - 1 ? (
          <button
            onClick={next}
            className="px-5 py-2.5 rounded-xl bg-[#c23c84] text-white text-sm font-semibold hover:bg-[#a02870] transition"
          >
            Næste →
          </button>
        ) : (
          <button
            onClick={submit}
            disabled={loading}
            className="px-5 py-2.5 rounded-xl bg-[#c23c84] text-white text-sm font-semibold hover:bg-[#a02870] transition disabled:opacity-60"
          >
            {loading ? "Indsender..." : "Indsend ansøgning"}
          </button>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Commit**

```bash
cd /root/esport-web-main
git add "app/hold/[slug]/ansoeg/page.tsx"
git commit -m "feat(hold): add multi-step season application form"
```

---

## Task 8: Admin-side + Deploy frontend

**Files:**
- Create: `/root/esport-web-main/app/admin/ansoegninger/page.tsx`
- Modify: `/root/esport-web-main/app/admin/page.tsx`

- [ ] **Step 1: Opret admin ansøgninger side**

```bash
mkdir -p /root/esport-web-main/app/admin/ansoegninger
```

Opret `/root/esport-web-main/app/admin/ansoegninger/page.tsx`:

```tsx
"use client";

import { useEffect, useState } from "react";

interface Question {
  id: string;
  type: "short_text" | "long_text" | "radio" | "checkbox";
  label: string;
  required: boolean;
  options: string[];
}
interface Step { id: string; title: string; questions: Question[] }
interface Poll {
  id: string; title: string; description: string; status: "DRAFT" | "OPEN" | "CLOSED";
  steps: Step[]; season?: { name: string }; _count?: { submissions: number }; seasonId?: string;
}
interface Submission {
  id: string; applicantType: "TEAM" | "ORG" | "SCHOOL"; status: "PENDING" | "CONFIRMED" | "REJECTED";
  answers: Record<string, any>; createdAt: string;
  team: { name: string; slug: string; logoUrl?: string };
  captain: { username: string; email: string };
}

const uid = () => Math.random().toString(36).slice(2, 8);
const TYPE_LABELS: Record<string, string> = { TEAM: "Hold", ORG: "Organisation", SCHOOL: "Efterskole/Institution" };
const STATUS_COLORS: Record<string, string> = {
  PENDING: "bg-yellow-900/40 text-yellow-400 border-yellow-800/40",
  CONFIRMED: "bg-green-900/40 text-green-400 border-green-800/40",
  REJECTED: "bg-red-900/40 text-red-400 border-red-800/40",
};
const POLL_STATUS_COLORS: Record<string, string> = {
  DRAFT: "bg-gray-900/40 text-gray-400 border-gray-700/40",
  OPEN: "bg-green-900/40 text-green-400 border-green-800/40",
  CLOSED: "bg-red-900/40 text-red-400 border-red-800/40",
};

export default function AdminApplicationsPage() {
  const [tab, setTab] = useState<"polls" | "submissions">("polls");
  const [polls, setPolls] = useState<Poll[]>([]);
  const [seasons, setSeasons] = useState<any[]>([]);
  const [msg, setMsg] = useState<{ text: string; ok: boolean } | null>(null);
  const [editing, setEditing] = useState<(Poll & { seasonId: string }) | null>(null);
  const [isNew, setIsNew] = useState(false);
  const [selectedPollId, setSelectedPollId] = useState("");
  const [submissions, setSubmissions] = useState<Submission[]>([]);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [loadingSubs, setLoadingSubs] = useState(false);

  const getToken = () => localStorage.getItem("token");
  const showMsg = (text: string, ok: boolean) => {
    setMsg({ text, ok });
    setTimeout(() => setMsg(null), 4000);
  };

  const fetchPolls = () => {
    fetch("https://api.esportserien.dk/admin/applications", {
      headers: { Authorization: `Bearer ${getToken()}` },
    }).then(r => r.json()).then(d => setPolls(Array.isArray(d) ? d : []));
  };

  useEffect(() => {
    fetchPolls();
    fetch("https://api.esportserien.dk/seasons")
      .then(r => r.json()).then(d => setSeasons(Array.isArray(d) ? d : []));
  }, []);

  const fetchSubmissions = (pollId: string) => {
    if (!pollId) return;
    setLoadingSubs(true);
    fetch(`https://api.esportserien.dk/admin/applications/${pollId}/submissions`, {
      headers: { Authorization: `Bearer ${getToken()}` },
    }).then(r => r.json())
      .then(d => setSubmissions(Array.isArray(d) ? d : []))
      .finally(() => setLoadingSubs(false));
  };

  const startNew = () => {
    setEditing({ id: "", title: "", description: "", status: "DRAFT", steps: [], seasonId: "" } as any);
    setIsNew(true);
  };
  const startEdit = (p: Poll) => { setEditing({ ...p, steps: JSON.parse(JSON.stringify(p.steps)), seasonId: p.seasonId ?? "" }); setIsNew(false); };

  const addStep = () => {
    if (!editing) return;
    setEditing({ ...editing, steps: [...editing.steps, { id: uid(), title: "Nyt trin", questions: [] }] });
  };
  const removeStep = (sid: string) => editing && setEditing({ ...editing, steps: editing.steps.filter(s => s.id !== sid) });
  const moveStep = (sid: string, dir: -1 | 1) => {
    if (!editing) return;
    const i = editing.steps.findIndex(s => s.id === sid);
    if ((dir === -1 && i === 0) || (dir === 1 && i === editing.steps.length - 1)) return;
    const steps = [...editing.steps];
    [steps[i], steps[i + dir]] = [steps[i + dir], steps[i]];
    setEditing({ ...editing, steps });
  };
  const updateStepTitle = (sid: string, title: string) =>
    editing && setEditing({ ...editing, steps: editing.steps.map(s => s.id === sid ? { ...s, title } : s) });
  const addQuestion = (sid: string) => {
    if (!editing) return;
    const q: Question = { id: uid(), type: "short_text", label: "Nyt spørgsmål", required: false, options: [] };
    setEditing({ ...editing, steps: editing.steps.map(s => s.id === sid ? { ...s, questions: [...s.questions, q] } : s) });
  };
  const removeQuestion = (sid: string, qid: string) =>
    editing && setEditing({ ...editing, steps: editing.steps.map(s => s.id === sid ? { ...s, questions: s.questions.filter(q => q.id !== qid) } : s) });
  const moveQuestion = (sid: string, qid: string, dir: -1 | 1) => {
    if (!editing) return;
    setEditing({ ...editing, steps: editing.steps.map(s => {
      if (s.id !== sid) return s;
      const i = s.questions.findIndex(q => q.id === qid);
      if ((dir === -1 && i === 0) || (dir === 1 && i === s.questions.length - 1)) return s;
      const qs = [...s.questions];
      [qs[i], qs[i + dir]] = [qs[i + dir], qs[i]];
      return { ...s, questions: qs };
    })});
  };
  const updateQuestion = (sid: string, qid: string, upd: Partial<Question>) =>
    editing && setEditing({ ...editing, steps: editing.steps.map(s => s.id === sid ? { ...s, questions: s.questions.map(q => q.id === qid ? { ...q, ...upd } : q) } : s) });
  const updateOptions = (sid: string, qid: string, raw: string) =>
    updateQuestion(sid, qid, { options: raw.split("\n").map(o => o.trim()).filter(Boolean) });

  const savePoll = async () => {
    if (!editing) return;
    if (!editing.title.trim()) { showMsg("Titel er påkrævet", false); return; }
    const body = { title: editing.title, description: editing.description, seasonId: editing.seasonId || undefined, steps: editing.steps, status: editing.status };
    const url = isNew ? "https://api.esportserien.dk/admin/applications" : `https://api.esportserien.dk/admin/applications/${editing.id}`;
    const res = await fetch(url, { method: isNew ? "POST" : "PUT", headers: { "Content-Type": "application/json", Authorization: `Bearer ${getToken()}` }, body: JSON.stringify(body) });
    if (res.ok) { showMsg(isNew ? "Poll oprettet" : "Poll gemt", true); fetchPolls(); setEditing(null); }
    else { const d = await res.json().catch(() => ({})); showMsg(d.message ?? "Fejl", false); }
  };

  const deletePoll = async (id: string) => {
    if (!confirm("Slet poll? Kan kun slettes hvis DRAFT og ingen ansøgninger.")) return;
    const res = await fetch(`https://api.esportserien.dk/admin/applications/${id}`, { method: "DELETE", headers: { Authorization: `Bearer ${getToken()}` } });
    if (res.ok) { showMsg("Poll slettet", true); fetchPolls(); }
    else { const d = await res.json().catch(() => ({})); showMsg(d.message ?? "Kan ikke slettes", false); }
  };

  const updateSubmission = async (pollId: string, subId: string, status: string) => {
    const res = await fetch(`https://api.esportserien.dk/admin/applications/${pollId}/submissions/${subId}`, {
      method: "PATCH", headers: { "Content-Type": "application/json", Authorization: `Bearer ${getToken()}` },
      body: JSON.stringify({ status }),
    });
    if (res.ok) {
      showMsg(status === "CONFIRMED" ? "Bekræftet — email sendt til captain" : "Afvist", true);
      setSubmissions(subs => subs.map(s => s.id === subId ? { ...s, status: status as any } : s));
    } else showMsg("Fejl", false);
  };

  return (
    <div className="max-w-4xl mx-auto space-y-6">
      <div>
        <h1 className="text-2xl font-bold">Ansøgninger</h1>
        <p className="text-sm opacity-40 mt-1">Administrer ansøgningspolls og se indkomne ansøgninger</p>
      </div>

      {msg && (
        <div className={`px-4 py-3 rounded-xl text-sm font-medium border ${msg.ok ? "bg-green-900/30 text-green-400 border-green-800/40" : "bg-red-900/30 text-red-400 border-red-800/40"}`}>
          {msg.text}
        </div>
      )}

      {/* Tabs */}
      <div className="flex gap-2">
        {(["polls", "submissions"] as const).map(t => (
          <button key={t} onClick={() => { setTab(t); setEditing(null); }}
            className={`px-4 py-2 rounded-xl text-sm font-semibold transition ${tab === t ? "bg-[#c23c84] text-white" : "bg-[var(--bg-secondary)] hover:opacity-80"}`}>
            {t === "polls" ? "Polls" : "Ansøgninger"}
          </button>
        ))}
      </div>

      {/* ── TAB: POLLS liste ── */}
      {tab === "polls" && !editing && (
        <div className="space-y-4">
          <button onClick={startNew} className="px-4 py-2 bg-[#c23c84] text-white rounded-xl text-sm font-semibold hover:bg-[#a02870] transition">
            + Ny poll
          </button>
          {polls.length === 0 && <div className="opacity-40 text-sm">Ingen polls endnu</div>}
          {polls.map(poll => (
            <div key={poll.id} className="bg-[var(--bg-secondary)] rounded-2xl p-5">
              <div className="flex items-start justify-between gap-3">
                <div className="space-y-1">
                  <div className="font-bold">{poll.title}</div>
                  {poll.description && <div className="text-xs opacity-50">{poll.description}</div>}
                  <div className="flex items-center gap-3 mt-1">
                    {poll.season && <span className="text-xs opacity-50">{poll.season.name}</span>}
                    <span className="text-xs opacity-50">{poll._count?.submissions ?? 0} ansøgninger</span>
                  </div>
                </div>
                <div className="flex items-center gap-2 flex-shrink-0">
                  <span className={`text-xs px-2 py-1 rounded-lg border font-semibold ${POLL_STATUS_COLORS[poll.status]}`}>{poll.status}</span>
                  <button onClick={() => startEdit(poll)} className="text-xs bg-[var(--bg-primary)] px-3 py-1.5 rounded-lg hover:bg-[var(--border)] transition font-semibold">Rediger</button>
                  <button onClick={() => deletePoll(poll.id)} className="text-xs text-red-400 px-3 py-1.5 rounded-lg hover:bg-red-900/20 transition font-semibold">Slet</button>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* ── POLL EDITOR ── */}
      {tab === "polls" && editing && (
        <div className="space-y-6">
          <div className="flex items-center justify-between">
            <h2 className="text-lg font-bold">{isNew ? "Ny poll" : "Rediger poll"}</h2>
            <button onClick={() => setEditing(null)} className="text-sm opacity-50 hover:opacity-100 transition">Annuller ✕</button>
          </div>

          {/* Meta */}
          <div className="bg-[var(--bg-secondary)] rounded-2xl p-5 space-y-4">
            <div className="space-y-1">
              <label className="text-xs font-semibold opacity-60 uppercase tracking-wide">Titel *</label>
              <input value={editing.title} onChange={e => setEditing({ ...editing, title: e.target.value })}
                className="w-full bg-[var(--bg-primary)] border border-[var(--border)] rounded-xl px-4 py-2.5 text-sm focus:border-[#c23c84] focus:outline-none transition" />
            </div>
            <div className="space-y-1">
              <label className="text-xs font-semibold opacity-60 uppercase tracking-wide">Beskrivelse</label>
              <textarea value={editing.description ?? ""} onChange={e => setEditing({ ...editing, description: e.target.value })} rows={2}
                className="w-full bg-[var(--bg-primary)] border border-[var(--border)] rounded-xl px-4 py-2.5 text-sm focus:border-[#c23c84] focus:outline-none transition resize-none" />
            </div>
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-1">
                <label className="text-xs font-semibold opacity-60 uppercase tracking-wide">Sæson</label>
                <select value={editing.seasonId ?? ""} onChange={e => setEditing({ ...editing, seasonId: e.target.value })}
                  className="w-full bg-[var(--bg-primary)] border border-[var(--border)] rounded-xl px-3 py-2.5 text-sm focus:border-[#c23c84] focus:outline-none transition">
                  <option value="">Ingen</option>
                  {seasons.map(s => <option key={s.id} value={s.id}>{s.name}</option>)}
                </select>
              </div>
              <div className="space-y-1">
                <label className="text-xs font-semibold opacity-60 uppercase tracking-wide">Status</label>
                <select value={editing.status} onChange={e => setEditing({ ...editing, status: e.target.value as any })}
                  className="w-full bg-[var(--bg-primary)] border border-[var(--border)] rounded-xl px-3 py-2.5 text-sm focus:border-[#c23c84] focus:outline-none transition">
                  <option value="DRAFT">DRAFT</option>
                  <option value="OPEN">OPEN</option>
                  <option value="CLOSED">CLOSED</option>
                </select>
              </div>
            </div>
          </div>

          {/* Steps */}
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <h3 className="font-semibold">Trin ({editing.steps.length})</h3>
              <button onClick={addStep} className="text-sm text-[#e05aa0] hover:underline">+ Tilføj trin</button>
            </div>
            {editing.steps.length === 0 && (
              <div className="text-sm opacity-40 text-center py-6 border border-dashed border-[var(--border)] rounded-2xl">
                Ingen trin endnu — klik "+ Tilføj trin"
              </div>
            )}
            {editing.steps.map((s, si) => (
              <div key={s.id} className="bg-[var(--bg-secondary)] border border-[var(--border)] rounded-2xl p-5 space-y-4">
                <div className="flex items-center gap-2">
                  <input value={s.title} onChange={e => updateStepTitle(s.id, e.target.value)}
                    className="flex-1 bg-[var(--bg-primary)] border border-[var(--border)] rounded-xl px-3 py-2 text-sm font-semibold focus:border-[#c23c84] focus:outline-none transition" />
                  <button onClick={() => moveStep(s.id, -1)} disabled={si === 0}
                    className="px-2 py-1 text-xs rounded-lg bg-[var(--bg-primary)] disabled:opacity-30 hover:bg-[var(--border)] transition">↑</button>
                  <button onClick={() => moveStep(s.id, 1)} disabled={si === editing.steps.length - 1}
                    className="px-2 py-1 text-xs rounded-lg bg-[var(--bg-primary)] disabled:opacity-30 hover:bg-[var(--border)] transition">↓</button>
                  <button onClick={() => removeStep(s.id)} className="px-2 py-1 text-xs rounded-lg text-red-400 hover:bg-red-900/20 transition">✕</button>
                </div>

                <div className="space-y-3 pl-3 border-l-2 border-[var(--border)]">
                  {s.questions.map((q, qi) => (
                    <div key={q.id} className="bg-[var(--bg-primary)] rounded-xl p-4 space-y-3">
                      <div className="flex items-start gap-2">
                        <input value={q.label} onChange={e => updateQuestion(s.id, q.id, { label: e.target.value })}
                          placeholder="Spørgsmål..."
                          className="flex-1 bg-transparent border-b border-[var(--border)] pb-1 text-sm focus:border-[#c23c84] focus:outline-none transition" />
                        <div className="flex gap-1 flex-shrink-0">
                          <button onClick={() => moveQuestion(s.id, q.id, -1)} disabled={qi === 0}
                            className="px-1.5 py-0.5 text-xs rounded bg-[var(--bg-secondary)] disabled:opacity-30 hover:bg-[var(--border)] transition">↑</button>
                          <button onClick={() => moveQuestion(s.id, q.id, 1)} disabled={qi === s.questions.length - 1}
                            className="px-1.5 py-0.5 text-xs rounded bg-[var(--bg-secondary)] disabled:opacity-30 hover:bg-[var(--border)] transition">↓</button>
                          <button onClick={() => removeQuestion(s.id, q.id)}
                            className="px-1.5 py-0.5 text-xs rounded text-red-400 hover:bg-red-900/20 transition">✕</button>
                        </div>
                      </div>
                      <div className="flex items-center gap-4 flex-wrap">
                        <select value={q.type} onChange={e => updateQuestion(s.id, q.id, { type: e.target.value as any })}
                          className="bg-[var(--bg-secondary)] border border-[var(--border)] rounded-lg px-2 py-1.5 text-xs focus:outline-none">
                          <option value="short_text">Kort svar</option>
                          <option value="long_text">Langt svar</option>
                          <option value="radio">Enkeltvalg</option>
                          <option value="checkbox">Flervalg</option>
                        </select>
                        <label className="flex items-center gap-1.5 text-xs cursor-pointer">
                          <input type="checkbox" checked={q.required} onChange={e => updateQuestion(s.id, q.id, { required: e.target.checked })} className="accent-[#c23c84]" />
                          Påkrævet
                        </label>
                      </div>
                      {(q.type === "radio" || q.type === "checkbox") && (
                        <div className="space-y-1">
                          <div className="text-xs opacity-50">Svarmuligheder (én per linje)</div>
                          <textarea value={(q.options ?? []).join("\n")} onChange={e => updateOptions(s.id, q.id, e.target.value)}
                            rows={3} placeholder={"Mulighed 1\nMulighed 2"}
                            className="w-full bg-[var(--bg-secondary)] border border-[var(--border)] rounded-lg px-3 py-2 text-xs focus:border-[#c23c84] focus:outline-none resize-none transition" />
                        </div>
                      )}
                    </div>
                  ))}
                  <button onClick={() => addQuestion(s.id)} className="text-xs text-[#e05aa0] hover:underline pl-1">
                    + Tilføj spørgsmål
                  </button>
                </div>
              </div>
            ))}
          </div>

          <button onClick={savePoll} className="w-full py-3 bg-[#c23c84] text-white rounded-xl font-semibold hover:bg-[#a02870] transition">
            {isNew ? "Opret poll" : "Gem ændringer"}
          </button>
        </div>
      )}

      {/* ── TAB: SUBMISSIONS ── */}
      {tab === "submissions" && (
        <div className="space-y-5">
          <div className="space-y-1">
            <label className="text-xs font-semibold opacity-60 uppercase tracking-wide">Vælg poll</label>
            <select value={selectedPollId} onChange={e => { setSelectedPollId(e.target.value); fetchSubmissions(e.target.value); setExpanded(null); }}
              className="bg-[var(--bg-secondary)] border border-[var(--border)] rounded-xl px-4 py-2.5 text-sm focus:border-[#c23c84] focus:outline-none transition w-full max-w-sm">
              <option value="">Vælg poll...</option>
              {polls.map(p => <option key={p.id} value={p.id}>{p.title} [{p.status}]</option>)}
            </select>
          </div>

          {loadingSubs && <div className="animate-pulse opacity-50 text-sm">Indlæser ansøgninger...</div>}

          {selectedPollId && !loadingSubs && (
            <>
              {/* Stats */}
              <div className="grid grid-cols-3 gap-3">
                {(["PENDING", "CONFIRMED", "REJECTED"] as const).map(s => (
                  <div key={s} className={`rounded-xl px-4 py-3 border text-center ${STATUS_COLORS[s]}`}>
                    <div className="text-2xl font-bold">{submissions.filter(sub => sub.status === s).length}</div>
                    <div className="text-xs opacity-70 mt-0.5">
                      {s === "PENDING" ? "Afventer" : s === "CONFIRMED" ? "Bekræftet" : "Afvist"}
                    </div>
                  </div>
                ))}
              </div>

              {submissions.length === 0 && <div className="opacity-40 text-sm">Ingen ansøgninger endnu</div>}

              {/* Submission cards */}
              <div className="space-y-3">
                {submissions.map(sub => {
                  const poll = polls.find(p => p.id === selectedPollId);
                  return (
                    <div key={sub.id} className="bg-[var(--bg-secondary)] rounded-2xl overflow-hidden">
                      <div
                        className="flex items-center justify-between px-5 py-4 cursor-pointer hover:bg-[var(--bg-secondary)]/80 transition select-none"
                        onClick={() => setExpanded(expanded === sub.id ? null : sub.id)}
                      >
                        <div className="flex items-center gap-3">
                          <div className="w-9 h-9 rounded-xl bg-gradient-to-br from-[#521649] to-[#c23c84] flex items-center justify-center text-xs font-bold flex-shrink-0 overflow-hidden">
                            {sub.team.logoUrl
                              ? <img src={`https://api.esportserien.dk${sub.team.logoUrl}`} className="w-full h-full object-cover" alt="" />
                              : sub.team.name[0]}
                          </div>
                          <div>
                            <div className="font-semibold text-sm">{sub.team.name}</div>
                            <div className="text-xs opacity-50">{sub.captain.username} · {new Date(sub.createdAt).toLocaleDateString("da-DK")}</div>
                          </div>
                        </div>
                        <div className="flex items-center gap-2">
                          <span className="text-xs px-2 py-0.5 rounded-lg bg-[var(--bg-primary)] border border-[var(--border)]">{TYPE_LABELS[sub.applicantType]}</span>
                          <span className={`text-xs px-2 py-0.5 rounded-lg border font-semibold ${STATUS_COLORS[sub.status]}`}>
                            {sub.status === "PENDING" ? "Afventer" : sub.status === "CONFIRMED" ? "Bekræftet" : "Afvist"}
                          </span>
                          <span className="text-xs opacity-30">{expanded === sub.id ? "▲" : "▼"}</span>
                        </div>
                      </div>

                      {expanded === sub.id && (
                        <div className="border-t border-[var(--border)] px-5 py-5 space-y-4">
                          {/* Answers */}
                          {poll && (poll.steps as Step[]).map(step => (
                            <div key={step.id} className="space-y-2">
                              <div className="text-xs font-semibold opacity-50 uppercase tracking-wide">{step.title}</div>
                              {step.questions.map(q => {
                                const ans = sub.answers[q.id];
                                return (
                                  <div key={q.id} className="bg-[var(--bg-primary)] rounded-xl px-4 py-3 space-y-1">
                                    <div className="text-xs opacity-50">{q.label}</div>
                                    <div className="text-sm">{Array.isArray(ans) ? ans.join(", ") : ans ?? "—"}</div>
                                  </div>
                                );
                              })}
                            </div>
                          ))}

                          {/* Actions */}
                          {sub.status === "PENDING" ? (
                            <div className="flex gap-3 pt-1">
                              <button onClick={() => updateSubmission(selectedPollId, sub.id, "CONFIRMED")}
                                className="flex-1 py-2.5 bg-green-700/30 text-green-400 border border-green-700/40 rounded-xl text-sm font-semibold hover:bg-green-700/50 transition">
                                Bekræft ✓
                              </button>
                              <button onClick={() => updateSubmission(selectedPollId, sub.id, "REJECTED")}
                                className="flex-1 py-2.5 bg-red-900/20 text-red-400 border border-red-800/40 rounded-xl text-sm font-semibold hover:bg-red-900/40 transition">
                                Afvis ✕
                              </button>
                            </div>
                          ) : (
                            <div className="text-xs opacity-40 text-center pt-1">
                              {sub.status === "CONFIRMED" ? "Bekræftet — email sendt til captain" : "Afvist"}
                            </div>
                          )}
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Tilføj link til admin-forsiden**

I `/root/esport-web-main/app/admin/page.tsx`, find `const links = [` arrayet og tilføj én linje efter `{ href: "/admin/news", ... }`:

```typescript
  { href: "/admin/ansoegninger", title: "Ansøgninger", desc: "Administrer ansøgningspolls og se svar" },
```

- [ ] **Step 3: Commit**

```bash
cd /root/esport-web-main
git add app/admin/ansoegninger/page.tsx app/admin/page.tsx
git commit -m "feat(admin): add application polls builder and submissions viewer"
```

- [ ] **Step 4: Deploy frontend**

```bash
cd /root/esport-prod
docker compose build frontend && docker compose up -d frontend
```

Vent ~60 sekunder (Next.js build tager tid). Verificér:
```bash
docker logs esport-prod-frontend-1 --tail 20
```

Forventet: ingen build-fejl, containeren kører.

- [ ] **Step 5: End-to-end verificering**

1. Gå til `https://esportserien.dk/admin/ansoegninger` — opret en OPEN poll med mindst ét trin og ét spørgsmål
2. Log ind som captain for et hold → gå til holdets side → se "Ansøg til sæson →"-knappen
3. Klik knappen → udfyld formularen → indsend
4. Gå tilbage til admin → tab "Ansøgninger" → vælg poll → se ansøgningen → klik "Bekræft ✓"
5. Verificér at captain modtager bekræftelses-email
