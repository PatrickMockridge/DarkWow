"""The one parser for doc/src/arch/verification-hazop.md.

Three instruments read the tracker: the gate (`scripts/check-defect-register.sh`), the status report
(`scripts/report-status.sh`) and the verification preflight (`scripts/check-tracker-settled.sh`).
They used to carry three different regexes, and the adversarial verifier found the drift: a
six-cell row with an unknown status was silently dropped by the gate's parser while the status
report could still count it. One parser, one vocabulary, one answer.

Vocabulary
    Main table: OPEN, BLOCKED, DEFERRED, CLOSED. The first three are UNDECIDED and block every
    verification surface. CLOSED is a decision only with a code change and a failing-before control
    (hazop-rules.md, "Definition of closed out").
    Appendix: `residual (R-n)`, `merged into OBL-X`, `decision` are live tombstones. `settled`,
    `restated`, `accepted`, `dissolved` are FROZEN history from before the tracker migration; they
    are not a disposition vocabulary and a new row may not use them. The gate hashes that frozen
    set through the approval manifest.
"""

import hashlib
import re

REGISTER_REL = "doc/src/arch/verification-hazop.md"
REGISTER = REGISTER_REL

STATUSES = ("OPEN", "BLOCKED", "DEFERRED", "CLOSED")
BLOCKING = ("OPEN", "BLOCKED", "DEFERRED")
SEV = {"C": 5, "H": 3, "M": 2, "L": 1}
LIK = {"D": 5, "P": 3, "T": 1}

HISTORICAL_RE = re.compile(r"^(settled|restated|accepted|dissolved)( \(.*\))?$")
TOMBSTONE_RE = re.compile(
    r"^(residual( \(R-\d+(, see residual-risk\.md)?\))?"
    r"|merged into OBL-[CZT]\d+"
    r"|decision( \(see decisions\.md\))?)$"
)
ID_RE = re.compile(r"OBL-[CZT]\d+")
CYCLE_BOUND_RE = re.compile(r"^by:(\d{4}-\d{2}-\d{2})$")


def band(sev, lik):
    """The ALARP band for a risk pair, or None when C/H would be Broadly acceptable."""
    if sev not in SEV or lik not in LIK:
        return None
    r = SEV[sev] * LIK[lik]
    if r >= 15:
        return "Intolerable"
    if r <= 5:
        return None if sev in ("C", "H") else "Broadly acceptable"
    return "Tolerable"


def risk_band(sev_lik):
    """Band for display, from the `S/L` cell. C/H at low R is Tolerable, never Broadly."""
    try:
        sev, lik = sev_lik.split("/")
        r = SEV[sev] * LIK[lik]
    except (ValueError, KeyError):
        return "unknown"
    if r >= 15:
        return "Intolerable"
    if r <= 5 and sev not in ("C", "H"):
        return "Broadly acceptable"
    return "Tolerable"


def parse_register(text, register_rel=REGISTER_REL):
    """Return (rows, appendix, errors).

    rows:     ID -> {status, defect, locus, control, sev, lik, line}
    appendix: ID -> disposition text
    errors:   schema errors, including an unknown status. An unknown status is never dropped.
    """
    rows, appendix, errors, seen = {}, {}, [], {}
    for lineno, line in enumerate(text.splitlines(), 1):
        if not line.startswith("| OBL-"):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        oid = cells[0].strip("` ")
        if not ID_RE.fullmatch(oid):
            continue
        if len(cells) == 6:
            status = cells[1]
            if status not in STATUSES:
                errors.append(
                    f"{register_rel}:{lineno}: {oid} status '{status}' is not "
                    "OPEN/BLOCKED/DEFERRED/CLOSED"
                )
                continue
            _, _, defect, locus, control, risk = cells
            if len(line) > 600:
                errors.append(
                    f"{register_rel}:{lineno}: {oid} row is {len(line)} chars (cap 600)"
                )
            if not defect:
                errors.append(f"{register_rel}:{lineno}: {oid} has no defect sentence")
            m = re.fullmatch(r"([CHML])/([DPT])", risk)
            if not m:
                errors.append(
                    f"{register_rel}:{lineno}: {oid} risk '{risk}' is not S/L "
                    "(C/H/M/L x D/P/T)"
                )
            elif band(m.group(1), m.group(2)) is None:
                errors.append(
                    f"{register_rel}:{lineno}: {oid} C/H risk cannot be Broadly acceptable"
                )
            if status == "OPEN":
                c = control.strip("` ").strip()
                if c in ("", "—"):
                    errors.append(
                        f"{register_rel}:{lineno}: {oid} OPEN row has no control or plan"
                    )
            if status == "BLOCKED" and "blocked:" not in control:
                errors.append(
                    f"{register_rel}:{lineno}: {oid} BLOCKED row does not name blocked:<reason>"
                )
            if status == "DEFERRED" and "deferred" not in control:
                errors.append(
                    f"{register_rel}:{lineno}: {oid} DEFERRED row does not name its deferral"
                )
            seen.setdefault(oid, []).append(lineno)
            rows[oid] = dict(
                status=status,
                defect=defect,
                locus=locus,
                control=control,
                sev=m.group(1) if m else "?",
                lik=m.group(2) if m else "?",
                line=lineno,
            )
        elif len(cells) == 2:
            appendix[oid] = cells[1]
        else:
            errors.append(
                f"{register_rel}:{lineno}: {oid} row has {len(cells)} cells "
                "(want 6, or a 2-cell tombstone)"
            )
    for oid, lines in seen.items():
        if len(lines) > 1:
            errors.append(f"{register_rel}: {oid} is defined by {len(lines)} tracker rows")
    return rows, appendix, errors


def parse_cycles(text, register_rel=REGISTER_REL):
    """Return (cycles, errors) from the `## Fix cycles` table.

    A cycle names an owner, a bound (`by:YYYY-MM-DD`) and the OPEN rows it covers. Every OPEN row
    is in exactly one cycle; a listed row that is not OPEN is a stale entry.
    """
    cycles, errors = [], []
    in_section = False
    for lineno, line in enumerate(text.splitlines(), 1):
        if line.startswith("## "):
            in_section = line.strip() == "## Fix cycles"
            continue
        if not in_section or not line.startswith("|"):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) != 4 or cells[0] in ("Cycle", "---") or set(cells[0]) <= set("-"):
            continue
        cycle, owner, bound, rowlist = cells
        if not owner or owner == "—":
            errors.append(f"{register_rel}:{lineno}: cycle '{cycle}' has no owner")
        if not CYCLE_BOUND_RE.fullmatch(bound):
            errors.append(
                f"{register_rel}:{lineno}: cycle '{cycle}' bound '{bound}' is not by:YYYY-MM-DD"
            )
        ids = [x.strip() for x in rowlist.split(",") if x.strip()]
        for oid in ids:
            if not ID_RE.fullmatch(oid):
                errors.append(
                    f"{register_rel}:{lineno}: cycle '{cycle}' lists '{oid}', not an OBL id"
                )
        cycles.append(dict(cycle=cycle, owner=owner, bound=bound, rows=ids, line=lineno))
    return cycles, errors


def check_cycles(rows, cycles, register_rel=REGISTER_REL):
    """Every OPEN row in exactly one cycle; no stale or lapsed entry."""
    from datetime import date

    errors = []
    covered = {}
    for c in cycles:
        m = CYCLE_BOUND_RE.fullmatch(c["bound"])
        if m and date.fromisoformat(m.group(1)) < date.today():
            errors.append(
                f"{register_rel}: cycle '{c['cycle']}' bound {c['bound']} has lapsed — "
                "close or residual every listed row, or record a new disposition"
            )
        for oid in c["rows"]:
            if oid not in rows:
                errors.append(
                    f"{register_rel}: cycle '{c['cycle']}' lists {oid}, which is not a live row"
                )
            elif rows[oid]["status"] != "OPEN":
                errors.append(
                    f"{register_rel}: cycle '{c['cycle']}' lists {oid}, which is "
                    f"{rows[oid]['status']}, not OPEN"
                )
            else:
                covered[oid] = covered.get(oid, 0) + 1
    for oid, row in rows.items():
        if row["status"] == "OPEN":
            n = covered.get(oid, 0)
            if n == 0:
                errors.append(f"{register_rel}: {oid} OPEN row is in no fix cycle")
            elif n > 1:
                errors.append(
                    f"{register_rel}: {oid} OPEN row is in {n} fix cycles"
                )
    return errors


def historical_baseline(appendix):
    """SHA-256 of the frozen pre-migration appendix rows.

    `settled`, `restated`, `accepted` and `dissolved` are history. Changing one of those rows —
    including adding a new one — changes this hash and fails the gate until the approval manifest
    is regenerated deliberately.
    """
    lines = sorted(
        f"{oid}\t{disp.strip()}"
        for oid, disp in appendix.items()
        if HISTORICAL_RE.fullmatch(disp.strip())
    )
    return hashlib.sha256(("\n".join(lines) + "\n").encode()).hexdigest()
