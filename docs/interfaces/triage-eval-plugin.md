# triage-eval-plugin: the first "eval-as-plugin" (`crates/plugin-triage-eval`)

- Authoritative source: the first client's real code, read by the client's own
  development session (`TriageAgent.cs:35-46`, `Domain/Entities/Service.cs`,
  `generate-rich-seed.py:129-134`, `seed-dev.sql:139,169,184,199`) and relayed
  with exact quotes, not examples invented for the occasion.
- Verification date: 2026-08-17
- Scope: the first part of an "evals without LangSmith" piece of work started by
  the client's team (discussion of 2026-08-17). The deterministic part only; the
  "semantic judgement" part (summary/compliance) was an open architecture
  question, settled in its own section below.

## Context: why this specific agent, and not summary/compliance

Three of the client's agents were analysed: triage, summary, compliance. Only one
has a real canonical reference to compare a categorical output against, which is
what allows a deterministic rule rather than a semantic judgement.

- **Triage** (`TriageAgent.cs:35-46`): the prompt asks for a medical specialty
  tag in French, with an **open** list ("for example: cardiologie, pédiatrie,
  neurologie, biologie, dermatologie, gynécologie, urgence, gériatrie, médecine
  générale"). The output is post-processed with `.Trim().ToLowerInvariant()`.
  But the **real** reference the client uses elsewhere (`Domain/Entities/Service.cs`)
  has only 6 seeded values (`generate-rich-seed.py:129-134`): `Cardiologie`,
  `Pédiatrie`, `Urgences`, `Gynécologie-Obstétrique`, `Dermatologie`,
  `Médecine générale`. **Drift confirmed in the database, not hypothetical**:
  `seed-dev.sql:139,169,184,199` has real rows `Call.AiTriageTag =
  'biologie'`/`'neurologie'` with no matching `Service`, and `urgence` (prompt,
  singular) does not even match `Urgences` (the real Service, plural) although the
  concept exists on both sides.
- **Summary/Compliance** (`SummaryAgent.cs:33-42`, `ComplianceAgent.cs:32-42`):
  free prose, no structured schema, no reference to compare with. A faithful
  summary or a correct GDPR audit cannot be checked by a deterministic rule on
  attributes. Out of this plugin's scope.

## Contract

**Input attribute** (set by the client; **not yet emitted as of 2026-08-17**, a
gap found while scoping this plugin, not assumed): `oncology.triage.tag`
(`String`), the already post-processed value (`.Trim().ToLowerInvariant()`) the
client stores today as `Call.AiTriageTag`. Without this attribute on the
`invoke_agent` span, `TriageEvalPlugin` is a no-op, the same stance as
`MedicalPlugin` facing a span without `oncology.current_step`.

**Output attribute**: `eval.triage.tag_known` (`Bool`), a new `eval.*`
namespace, distinct from `oncology.*` (raw data set by the client), to separate
clearly "what the client produced" from "what Retina inferred from it". Proposed
by the client's session and kept as is.

**Comparison**: `tag_known = true` if the tag (normalised with trim + lowercase,
the same normalisation the client already applies) exactly matches a known
service name (normalised the same way). No fuzzy or partial match: an
approximate match would silently hide real vocabulary drift, the opposite of what
this plugin must detect.

**Warning**: if `tag_known = false`, a `plugin.warning` is attached, visible
through `spans_with_warnings` (`query-api::/metrics/summary`), the same
monitoring mechanism `MedicalPlugin` already uses.

## Source of the canonical reference

Decided with the owner (not frozen in code the way `MedicalPlugin` is):
`TriageEvalPlugin::new(known_services)` takes the list as a parameter rather than
hard-coding it. `crates/kernel/src/main.rs` resolves it from
`TRIAGE_KNOWN_SERVICES` (comma-separated, the same style as `ENABLED_PLUGINS`).
Unset means `DEFAULT_KNOWN_SERVICES` (the 6 real seeded values above), not an
empty list (an empty list would fail every tag, worse than a default based on
real data). This follows changes to the client's `Service` reference without
recompiling the kernel: a different choice from `MedicalPlugin` (a fixed field
list), justified because a business vocabulary drifts over time while a
HIPAA/GDPR governance schema does not move at the same pace.

## Part 2 (semantic judgement of summary/compliance): settled, no code in Retina

Decided with the owner (2026-08-17), after a first idea was set aside: a separate
binary (`eval-worker`) that would read agent transcripts and outputs back from
ClickHouse to call an LLM judge was considered, then rejected.
`docs/interfaces/clickhouse-schema.md` already establishes that potentially
sensitive attributes are **opt-in, off by default**, precisely so as not to store
patient data in clear in Retina. The real `ComplianceAgent` case (name, date of
birth, social security number, HIV status) would make that risk concrete, not
theoretical, if the source text went through Retina to be judged.

**The decision**: the semantic judge runs **on the client side** (this client or
any future one), with its own text, its own API key, its own budget, never sent to
Retina. Only the **structured verdict** is posted as an attribute on the
`invoke_agent` span, in the `eval.*` namespace (like `eval.triage.tag_known`
above), with a typed value (bool/int/float), never free text as a verdict. This is
consistent with why `ComplianceAgent` itself has no structured gate today
(`oncology-governance.md`): a verdict in prose cannot be queried, a structured one
can.

**Consequence**: no new crate, table, migration or API key in Retina for this
part. `extra_attributes` (`Map(String, String)`, already generic) absorbs
`eval.summary.*`/`eval.compliance.*` exactly like `oncology.*` today. It
generalises better than a central `eval-worker`: no coupling of Retina to an LLM
provider or to a per-client text format, consistent with today's single-tenant
model (ADR-0001) rather than adding a multi-client responsibility. A convention
to share with each client that wants to use it, not a contract to evolve in the
kernel.

## Verified

7 unit tests (`crates/plugin-triage-eval`): a known tag (the real case
"pédiatrie") → `tag_known=true` with no warning; a drifted tag already seen in the
client's data ("biologie") → `tag_known=false` + a warning; `urgence` vs
`Urgences` (the same concept, singular/plural mismatch) → `tag_known=false`; case
and whitespace normalisation; tag absent → no-op; a non-`AgentRun` event → no-op.
Plus 2 tests on `triage_known_services` (`crates/kernel`): the default is the real
list, an environment override is split and trimmed. **Not yet verified in real
conditions** (unlike `cost_usd`): blocked on `oncology.triage.tag`, still not
emitted by the client at the time of writing.
