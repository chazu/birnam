#!/usr/bin/env python3
"""Bounded, single-worker TLC checks + fresh Lean kernel checks and mutations.

No downloads unless --fetch-tools is given. Results live under .cargo-target.
This verifies the abstract contract, not the generated C or current Root owner.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
FORMAL = ROOT / "formal/rooting"
SEMANTICS = ROOT / "formal/semantics"
CACHE = ROOT / ".cargo-target/formal"
ALLOWED_AXIOMS = {"propext", "Classical.choice", "Quot.sound"}
CASES = {
    'DriveSafe': None,
    'DriveEpoch': 'Invariant BalancedReentry is violated.',
    'DriveAction': 'Invariant OldActionsStayStale is violated.',
    'DriveScope': 'Invariant NewScopeInvalidates is violated.',
    'DriveFrame': 'Invariant ChangedFrameInvalidates is violated.',
    'DriveLease': 'Invariant RevokedLeaseInvalidates is violated.',

    'OwnedSafe': None,
    'OwnedLost': 'Invariant ForeignSurvives is violated.',
    'OwnedForeign': 'Invariant ForeignSurvives is violated.',
    'OwnedCredits': 'Invariant CreditsConsumed is violated.',
    'OwnedImage': 'Invariant ImageHasNoBank is violated.',
    'OwnedConstruction': 'Invariant FreshDoesNotInvalidate is violated.',
    'OwnedABA': 'Invariant ExistingABAInvalidates is violated.',

    "RegistrySafe": None,
    "RegistryAbort": "Invariant NoStrandedTransition is violated.",
    "RegistryLost": "Invariant CompletedEditsSurvive is violated.",
    "RegistryField": "Invariant CompletedEditsSurvive is violated.",
    "SearchSafe": None,
    "SearchReplay": "Invariant SearchContract is violated.",
    "SearchStale": "Invariant SearchContract is violated.",
    "SearchRoot": "Invariant SearchContract is violated.",
    "SearchNil": "Invariant SearchContract is violated.",

    "TablesSmall": None,
    "TablesLarge": None,
    "TablesKey": "Invariant CurrentNameAndDispatch is violated.",
    "TablesFull": "Invariant CurrentNameAndDispatch is violated.",
    "TablesWorld": "Invariant CurrentNameAndDispatch is violated.",
    "HelperSafe": None,
    "HelperTransitive": "Invariant SameCalls is violated.",
    "HelperAddress": "Invariant SameCalls is violated.",
    "HelperDispatch": "Invariant SameCalls is violated.",
    "SlotSafe": None,
    "SlotKey": "Invariant CurrentLookup is violated.",
    "SlotValue": "Invariant CurrentLookup is violated.",
    "SlotParent": "Invariant CurrentLookup is violated.",

    "RelationalSafe": None,
    "RelationalCached": "Invariant CurrentComparison is violated.",
    "RelationalShared": "Invariant SnapshotsAlive is violated.",
    "RelationalExpired": "Invariant ClosedCollectors is violated.",
    "RelationalFailure": "Invariant FailuresLatched is violated.",
    "RelationalReplay": "Invariant CurrentComparison is violated.",

    "IndexedSafe": None,
    "IndexedBounds": "Invariant AllValidated is violated.",
    "IndexedReplay": "Invariant CurrentCallbacks is violated.",
    "IndexedStale": "Invariant CurrentCallbacks is violated.",
    "IndexedEarly": "Invariant InventoryAlive is violated.",
    "ShapesSafe": None,
    "ShapesSecond": "Invariant CertifiedBatch is violated.",
    "ShapesServices": "Invariant CertifiedBatch is violated.",
    "ShapesOwn": "Invariant CertifiedBatch is violated.",
    "ShapesDelay": "Invariant CertifiedBatch is violated.",
    "ShapesOrder": "Invariant OriginalOrder is violated.",
    "FinishSafe": None,
    "FinishMissing": "Invariant ResultAlive is violated.",
    "FinishEarly": "Invariant ResultAlive is violated.",
    "FinishOrder": "Invariant ReleaseOrder is violated.",
    "Safe": None,
    "BranchSafe": None,
    "BranchBoth": "Invariant OnlyChosen is violated.",
    "BranchStale": "Invariant CurrentEntry is violated.",
    "BranchArguments": "Invariant NeededAlive is violated.",
    "BranchReplay": "Invariant NoReplay is violated.",
    "LocalSafe": None,
    "LocalUnrooted": "Invariant NeededAlive is violated.",
    "LocalEviction": "Invariant NeededAlive is violated.",
    "LocalStale": "Invariant NeededAlive is violated.",
    "LocalEdge": "Invariant NeededAlive is violated.",
    "LocalEarly": "Invariant NeededAlive is violated.",
    "LocalLeak": "Invariant NoLeakedProtection is violated.",
    "ResolveSafe": None,
    "ResolveNil": "Invariant NeededAlive is violated.",
    "ResolveGlobals": "Invariant NeededAlive is violated.",
    "ResolveReboundNil": "Invariant GetterOrder is violated.",
    "ResolveEarly": "Invariant NeededAlive is violated.",
    "ResolveOrder": "Invariant GetterOrder is violated.",
    "ShapeSafe": None,
    "ShapeGetter": "Invariant CertifiedRead is violated.",
    "ShapeOwn": "Invariant CertifiedRead is violated.",
    "ShapeCached": "Invariant CertifiedRead is violated.",
    "ShapeDelay": "Invariant CertifiedRead is violated.",
    "ShapeOrder": "Invariant OriginalFallbackOrder is violated.",
    "SnapshotSafe": None,
    "SnapshotNode": "Invariant NeededAlive is violated.",
    "SnapshotInputs": "Invariant NeededAlive is violated.",
    "SnapshotEarly": "Invariant NeededAlive is violated.",
    "OwnerSafe": None,
    "OwnerReport": "Invariant NeededAlive is violated.",
    "OwnerEarly": "Invariant NeededAlive is violated.",
    "OwnerNested": "Invariant NeededAlive is violated.",
    "OwnerLeak": "Invariant NoLeakedScope is violated.",
    "OwnerGap": "Invariant NeededAlive is violated.",
    "BulkSafe": None,
    "BulkAdmission": "Invariant BorrowOnlyCurrent is violated.",
    "BulkCopy": "Invariant SameOrderAndAliases is violated.",
    "BulkOrder": "Invariant SameOrderAndAliases is violated.",
    "BulkScope": "Invariant NeededAlive is violated.",
    "CaptureSafe": None,
    "CaptureFresh": "Invariant NeededAlive is violated.",
    "CaptureGrant": "Invariant NeededAlive is violated.",
    "CaptureScope": "Invariant NeededAlive is violated.",
    "CaptureEdge": "Invariant NeededAlive is violated.",
    "CaptureCached": "Invariant NeededAlive is violated.",
    "InvocationSafe": None,
    "InvocationUncovered": "Invariant NeededAlive is violated.",
    "InvocationEarlyExit": "Invariant NeededAlive is violated.",
    "InvocationMissingResult": "Invariant NeededAlive is violated.",
    "InvocationReturnGap": "Invariant NeededAlive is violated.",
    "InvocationLostCode": "Invariant NeededAlive is violated.",

    "AdmissionSafe": None,
    "AdmissionContext": "Invariant SameGuard is violated.",
    "AdmissionServices": "Invariant SameGuardCallbacks is violated.",
    "AdmissionPermission": "Invariant CurrentPermission is violated.",
    "AdmissionEpoch": "Invariant CurrentEntry is violated.",
    "RegionAddition": None,
    "RegionString": None,
    "RegionCollection": None,
    "RegionStale": "Invariant CurrentMeaning is violated.",
    "RegionPermission": "Invariant CurrentMeaning is violated.",
    "RegionExposure": "Invariant RecoveredBeforeExposure is violated.",
    "RegionReplay": "Invariant RecoveredBeforeExposure is violated.",
    "RegionCodeLease": "Invariant EnteredCodeAlive is violated.",
    "RegionOwner": "Invariant OwnerBoundary is violated.",
    "RegistrationSafe": None,
    "RegistrationUnrooted": "Invariant InputsAlive is violated.",
    "RegistrationPartial": "Invariant ExactRegistrations is violated.",
    "BatchSafe": None,
    "BorrowSafe": None,
    "BorrowCallback": "Invariant SameInspection is violated.",
    "BatchStale": "Invariant ExactCustomCallbacks is violated.",
    "BatchOrder": "Invariant ExactRetainedOrder is violated.",
    "Canonical": None,
    "Graph": None,
    "Nested": None,
    "Recovery": None,
    "OmitFirst": "Invariant RootCoverage is violated.",
    "EarlyRelease": "Invariant RootCoverage is violated.",
    "IgnorePermission": "Invariant ObservableEquality is violated.",
    "BadRevoke": "Invariant ObservableEquality is violated.",
    "LeakExit": "Invariant ReachabilityEquality is violated.",
    "UnfairRecovery": "Temporal properties were violated.",
}


def digest(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def checked_download(url, dest, sha):
    if dest.exists() and digest(dest) == sha:
        return
    partial = dest.with_suffix(dest.suffix + ".download")
    print(f"Downloading {dest.name}", flush=True)
    deadline = time.monotonic() + 180
    try:
        with urllib.request.urlopen(url, timeout=30) as response, partial.open("wb") as f:
            while chunk := response.read(1024 * 1024):
                if time.monotonic() > deadline:
                    raise TimeoutError("tool download exceeded 180 seconds")
                f.write(chunk)
        if digest(partial) != sha:
            raise RuntimeError(f"checksum mismatch: {dest.name}")
        partial.replace(dest)
    finally:
        partial.unlink(missing_ok=True)


def run(command, log, cwd=ROOT, env=None, timeout=30):
    start = time.monotonic()
    # Low priority and one TLC worker; no idle-machine requirement.
    result = subprocess.run(command, cwd=cwd, env=env, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            timeout=timeout, check=False)
    log.write_text(result.stdout)
    return result, round(time.monotonic() - start, 3)


def tools(args):
    lock = json.loads((FORMAL / "tools.json").read_text())
    cache = CACHE / "tools"
    cache.mkdir(parents=True, exist_ok=True)
    jar = Path(args.tla_jar).resolve() if args.tla_jar else cache / "tla2tools-1.7.4.jar"
    lean = Path(args.lean).resolve() if args.lean else None
    if args.fetch_tools and not args.tla_jar:
        checked_download(lock["tla"]["url"], jar, lock["tla"]["sha256"])
    if not jar.is_file() or digest(jar) != lock["tla"]["sha256"]:
        raise RuntimeError("missing/wrong pinned TLC jar; run with --fetch-tools")
    if lean is None:
        key = f"{platform.system()}-{platform.machine()}"
        archive = lock["lean"]["archives"].get(key)
        if archive is None:
            raise RuntimeError(f"no pinned Lean archive for {key}; supply --lean")
        dest = cache / archive["name"]
        lean = cache / archive["name"].removesuffix(".tar.zst") / "bin/lean"
        if args.fetch_tools and not lean.is_file():
            checked_download(lock["lean"]["base_url"] + archive["name"], dest, archive["sha256"])
            if not shutil.which("unzstd"):
                raise RuntimeError("unzstd is needed to extract the pinned Lean archive")
            subprocess.run(["tar", "--use-compress-program=unzstd", "-xf", str(dest),
                            "-C", str(cache)], check=True, timeout=90)
    if not lean.is_file():
        raise RuntimeError("missing Lean; run with --fetch-tools or supply --lean")
    java = args.java or shutil.which("java")
    if not java:
        raise RuntimeError("Java 17+ is needed for TLC")
    version = subprocess.check_output([str(lean), "--version"], text=True, timeout=10).strip()
    if not version.startswith(f"Lean (version {lock['lean']['version']},"):
        raise RuntimeError(f"wrong Lean version: {version}")
    return jar, lean, java, version


def check_tla(jar, java, out):
    results = {}
    for name, expected_failure in CASES.items():
        log = out / f"tlc-{name}.log"
        cmd = [java, "-XX:ActiveProcessorCount=1", "-XX:+UseParallelGC", "-Xmx512m",
               "-cp", str(jar), "tlc2.TLC", "-workers", "1", "-seed", "1", "-fp", "1",
               "-metadir", str(out / f"states-{name}"),
               "-config", str((SEMANTICS if name.startswith(("Region", "Admission", "Indexed", "Relational", "Slot", "Search", "Registry", "Owned", "Drive")) else FORMAL) / f"tla/{name}.cfg"),
               str(SEMANTICS / "tla/RecoveryDrive.tla") if name.startswith("Drive") else str(SEMANTICS / "tla/GlobalsOwner.tla") if name.startswith("Owned") else str(SEMANTICS / "tla/ModuleRegistry.tla") if name.startswith("Registry") else str(SEMANTICS / "tla/CollectionSearch.tla") if name.startswith("Search") else str(FORMAL / "tla/SelectorTables.tla") if name.startswith("Tables") else str(FORMAL / "tla/HelperEmission.tla") if name.startswith("Helper") else str(SEMANTICS / "tla/SlotHints.tla") if name.startswith("Slot") else str(SEMANTICS / "tla/RelationalQuery.tla") if name.startswith("Relational") else str(SEMANTICS / "tla/IndexedSnapshot.tla") if name.startswith("Indexed") else str(SEMANTICS / "tla/BindingAdmission.tla") if name.startswith("Admission") else str(SEMANTICS / "tla/ExecutionRegion.tla") if name.startswith("Region") else str(FORMAL / ("tla/ResultHandoff.tla" if name.startswith("Finish") else "tla/GlobalResolution.tla" if name.startswith("Resolve") else "tla/RepresentationBatch.tla" if name.startswith("Shapes") else "tla/RepresentationChecks.tla" if name.startswith("Shape") else "tla/BorrowedSnapshots.tla" if name.startswith("Snapshot") else "tla/NativeOwnerScopes.tla" if name.startswith("Owner") else "tla/BulkCaptures.tla" if name.startswith("Bulk") else "tla/ReusableBranches.tla" if name.startswith("Branch") else "tla/LocalReuse.tla" if name.startswith("Local") else "tla/CaptureReads.tla" if name.startswith("Capture") else "tla/InvocationInputs.tla" if name.startswith("Invocation") else "tla/RegistrationBatch.tla" if name.startswith("Registration") else "tla/BatchCleanup.tla" if name.startswith("Batch") else "tla/BorrowInspection.tla" if name.startswith("Borrow") else "tla/DuplicateRoots.tla"))]
        result, seconds = run(cmd, log)
        text = result.stdout
        if expected_failure:
            ok = result.returncode != 0 and f"Error: {expected_failure}" in text
            ok &= "The behavior up to this point is:" in text or "counter-example" in text.lower()
        else:
            ok = result.returncode == 0 and "Model checking completed. No error has been found." in text
            ok &= "0 states left on queue." in text and "Error:" not in text
        if not ok:
            raise RuntimeError(f"TLC {name} did not give the required result; see {log}")
        counts = re.search(r"([\d,]+) states generated, ([\d,]+) distinct states found", text)
        results[name] = {"expected_violation": expected_failure, "seconds": seconds,
                         "generated_states": int(counts[1].replace(",", "")) if counts else None,
                         "distinct_states": int(counts[2].replace(",", "")) if counts else None,
                         "log": log.name}
        print(f"TLC {name}: {'expected counterexample' if expected_failure else 'pass'} ({seconds}s)", flush=True)
    return results


def lean_theorems():
    names = []
    for module, namespace in [("Rooting", "Birnam.Rooting"), ("Examples", "Birnam.Rooting.Examples"), ("GC", "Birnam.GC"), ("Memo", "Birnam.Memo"), ("Borrow", "Birnam.Borrow"), ("Registration", "Birnam.Registration"), ("Invocation", "Birnam.Invocation"), ("Capture", "Birnam.Capture"), ("Selection", "Birnam.Selection"), ("LocalReuse", "Birnam.LocalReuse"), ("SymbolNames", "Birnam.SymbolNames"), ("Branches", "Birnam.Branches"), ("HelperEmission", "Birnam.HelperEmission"), ("Semantics", "Birnam.Semantics"), ("SemanticsExamples", "Birnam.SemanticsExamples"), ("Admission", "Birnam.Admission"), ("IndexedSnapshot", "Birnam.IndexedSnapshot"), ("Relational", "Birnam.Relational"), ("SlotHints", "Birnam.SlotHints"), ("JsonStrings", "Birnam.JsonStrings"), ("JsonDispatch", "Birnam.JsonDispatch"), ("JsonNumbers", "Birnam.JsonNumbers"), ("RangeHandlers", "Birnam.RangeHandlers"), ("LogMessages", "Birnam.LogMessages"), ("CollectionSearch", "Birnam.CollectionSearch"), ("StringOrder", "Birnam.StringOrder"), ("PrivateScratch", "Birnam.PrivateScratch"), ("SlotReflection", "Birnam.SlotReflection"), ("ModuleRegistry", "Birnam.ModuleRegistry"), ("GlobalsBinding", "Birnam.GlobalsBinding"), ("PrivateIndex", "Birnam.PrivateIndex")]:
        text = ((SEMANTICS if module in {"Semantics", "SemanticsExamples", "Admission", "IndexedSnapshot", "Relational", "SlotHints", "JsonStrings", "JsonDispatch", "JsonNumbers", "RangeHandlers", "LogMessages", "CollectionSearch", "StringOrder", "PrivateScratch", "SlotReflection", "ModuleRegistry", "GlobalsBinding", "PrivateIndex"} else FORMAL) / f"lean/{module}.lean").read_text()
        names.extend(f"{namespace}.{name}" for name in re.findall(r"^theorem (\w+)", text, re.M))
        # The kernel audit below is authoritative; this additionally rejects
        # accidental proof holes or unsafe evaluator shortcuts in source.
        without_comments = re.sub(r"/\-.*?\-/|--[^\n]*", "", text, flags=re.S)
        if re.search(r"\b(sorry|admit|axiom|unsafe|native_decide)\b", without_comments):
            raise RuntimeError(f"proof hole/custom axiom/unsafe shortcut in {module}")
    return names


def check_lean(lean, out):
    build = out / "lean"
    build.mkdir()
    env = dict(os.environ, LEAN_PATH=str(build))
    for module in ["Rooting", "Examples", "GC", "Memo", "Borrow", "Registration", "Invocation", "Capture", "Selection", "LocalReuse", "SymbolNames", "Branches", "HelperEmission", "Semantics", "SemanticsExamples", "Admission", "IndexedSnapshot", "Relational", "SlotHints", "JsonStrings", "JsonDispatch", "JsonNumbers", "RangeHandlers", "LogMessages", "CollectionSearch", "StringOrder", "PrivateScratch", "SlotReflection", "ModuleRegistry", "GlobalsBinding", "PrivateIndex"]:
        result, _ = run([str(lean), "-DwarningAsError=true", str((SEMANTICS if module in {"Semantics", "SemanticsExamples", "Admission", "IndexedSnapshot", "Relational", "SlotHints", "JsonStrings", "JsonDispatch", "JsonNumbers", "RangeHandlers", "LogMessages", "CollectionSearch", "StringOrder", "PrivateScratch", "SlotReflection", "ModuleRegistry", "GlobalsBinding", "PrivateIndex"} else FORMAL) / f"lean/{module}.lean"),
                         "-o", str(build / f"{module}.olean")], out / f"lean-{module}.log", env=env)
        if result.returncode:
            raise RuntimeError(f"Lean {module} failed; see {out / f'lean-{module}.log'}")
    names = lean_theorems()
    # Ensure the tracked audit lists every theorem, including counterexamples.
    audit = (FORMAL / "lean/Audit.lean").read_text() + (SEMANTICS / "lean/SemanticsAudit.lean").read_text()
    if set(re.findall(r"^#print axioms (\S+)$", audit, re.M)) != set(names):
        raise RuntimeError("Audit.lean must list every theorem")
    audit_file=build/"CombinedAudit.lean"
    audit_lines=audit.splitlines()
    audit_file.write_text("\n".join(line for line in audit_lines if line.startswith("import "))+"\n"+"\n".join(line for line in audit_lines if not line.startswith("import "))+"\n")
    result, _ = run([str(lean), "-DwarningAsError=true", str(audit_file)],
                     out / "lean-axioms.log", env=env)
    if result.returncode:
        raise RuntimeError("Lean axiom audit failed")
    reported = re.findall(r"'([^']+)' (?:depends on axioms: \[([^\]]*)\]|does not depend on any axioms)", result.stdout)
    if {name for name, _ in reported} != set(names):
        raise RuntimeError("incomplete kernel axiom audit")
    for name, raw in reported:
        used = {a.strip() for a in raw.split(",") if a.strip()}
        if not used <= ALLOWED_AXIOMS:
            raise RuntimeError(f"unapproved axiom dependencies in {name}: {used}")
    source = (FORMAL / "lean/Rooting.lean").read_text()
    mutations = {
        "omit-first": ("if optimized then (if p = 0 then 1 else p) else p + 1",
                       "if optimized then (if p = 0 then 0 else p) else p + 1"),
        "leak-on-exit": ("physical := fun s o => if s = scope then 0 else st.physical s o",
                         "physical := st.physical"),
        "skip-materialization": ("{st with physical := st.logical, optimized := fun _ => false}",
                                 "{st with physical := st.physical, optimized := fun _ => false}"),
    }
    extra_mutations = {
        "owner-cas-overwrites-new-root": ("GlobalsBinding", "if current = expected then replacement else current", "if current = expected then replacement else replacement"),
        "owner-image-keeps-credits": ("GlobalsBinding", "{ owner with credits := fun _ => 0 }", "owner"),
        "owner-name-uses-value-equality": ("GlobalsBinding", "a.identity == b.identity", "a.spelling == b.spelling"),
        "binding-nil-is-absent": ("GlobalsBinding", "| some value => [value]", "| some value => if value = 0 then [] else [value]"),
        "binding-inverse-overwrites-conflict": ("GlobalsBinding", "if bindings name = expected then put bindings name previous else bindings", "if bindings name = expected then put bindings name previous else put bindings name previous"),
        "private-index-confuses-collisions": ("PrivateIndex", "if name = key then some value else lookup key rest", "if True then some value else lookup key rest"),
        "private-index-reorders-roots": ("PrivateIndex", "⟨s.roots, hash⟩", "⟨s.roots.reverse, hash⟩"),
        "scratch-overwrites-public-world": ("PrivateScratch", "⟨s.world, put s.cells key value⟩", "⟨fun _ => value, put s.cells key value⟩"),
        "scratch-drops-catalog-publication": ("PrivateScratch", "⟨put s.world key value, s.cells⟩", "⟨s.world, s.cells⟩"),
        "default-cell-publishes-facts": ("PrivateScratch", "def defaultCell (identity : Nat) : CellHandle := .local identity", "def defaultCell (identity : Nat) : CellHandle := .fact identity"),
        "cell-reads-wrong-slot": ("PrivateScratch", "if slot = 0 then s.cells identity else 0", "if slot = 1 then s.cells identity else 0"),
        "registry-declaration-clears-loaded": ("ModuleRegistry", "write (write (write s name 0 1) name 1 1) name 2 dependencies", "write (write (write (write s name 0 1) name 1 1) name 2 dependencies) name 3 0"),
        "registry-write-publishes-facts": ("ModuleRegistry", "{ s with records := put s.records name (put (s.records name) field (some value)) }", "{ records := put s.records name (put (s.records name) field (some value)), facts := put s.facts name (put (s.facts name) field (some value)) }"),
        "mirror-writes-wrong-slot": ("SlotReflection", "slots := put s.slots index value", "slots := put s.slots (index + 1) value"),
        "mirror-rename-moves-values": ("SlotReflection", "names := put s.names index (some name)", "names := put s.names index (some name), slots := put s.slots index name"),
        "ordering-replays-byte": ("StringOrder", "| some answer => (next, answer, [index])", "| some answer => (next, answer, [index, index])"),
        "ordering-drops-effects": ("StringOrder", "| some answer => (next, answer, [index])", "| some answer => (state, answer, [index])"),
        "search-replays-hit": ("CollectionSearch", "| some answer => (next, some answer, [each])", "| some answer => (next, some answer, [each, each])"),
        "search-drops-effects": ("CollectionSearch", "| some answer => (next, some answer, [each])", "| some answer => (state, some answer, [each])"),
        "cached-digest-wrong-name": ("SymbolNames", "digest row.1 == cache name", "digest row.1 == cache 0"),
        "digest-collision-aliases-name": ("SymbolNames", "if key = name then some symbol else lookupName name rest", "if True then some symbol else lookupName name rest"),
        "log-keeps-seven": ("LogMessages", "if xs.length > 6 then xs.drop (xs.length - 6) else xs", "if xs.length > 6 then xs.drop (xs.length - 7) else xs"),
        "range-wrong-end-comparison": ("RangeHandlers", "let handler := if compare value.1 last > 0 then", "let handler := if compare value.1 last >= 0 then"),
        "json-unsafe-short-bound": ("JsonNumbers", "short : bytes < 19", "short : bytes < 20"),
        "json-ignore-uppercase-exponent": ("JsonNumbers", "if upper then number else integer", "if upper then integer else integer"),
        "json-wrong-kind-handler": ("JsonDispatch", "if kind = .number then handle .number else\n", "if kind = .number then handle .string else\n"),
        "json-skip-backslash-scan": ("JsonStrings", "if 92 ∈ bytes then decode false bytes else bytes", "if False then decode false bytes else bytes"),
        "json-reuse-nonempty-identity": ("JsonStrings", "if bytes.isEmpty then input else fresh", "if bytes.isEmpty then input else (fresh - fresh + input)"),
        "lose-full-cache-fallback": ("SymbolNames", "| none => some (canonical name)", "| none => none"),
        "skip-retained-helper-body": ("HelperEmission", "rest (body f state)", "rest state"),
        "charge-whole-capacity-again": ("GC", "before + (after - before)", "before + after"),
        "unchecked-slot-hint": ("SlotHints", "if name = key then some value else lookup key rows", "if true then some value else lookup key rows"),
        "freeze-relational-comparison": ("Relational", ".ok (s == 0)", ".ok true"),
        "reverse-indexed-edge": ("IndexedSnapshot", "(object a, object b)", "(object b, object a)"),
        "ignore-second-batched-entry": ("Admission", "def packedPair (a b : Facts) : Bool := packed a && packed b", "def packedPair (a _b : Facts) : Bool := packed a"),
        "wrong-direct-integer-operation": ("Branches", "def directIntegerSelection (op : Operation) : Option Operation := some op", "def directIntegerSelection (_op : Operation) : Option Operation := some .add"),
        "float-remainder-handler": ("Branches", "match op with | .remainder => none | other => some other", "some op"),
        "ignore-metadata-current": ("Admission", "if current then\n    if shapeMatches then", "if true then\n    if shapeMatches then"),
        "accept-unknown-metadata-identity": ("Admission", "if identity = 0 then none else some identity", "some identity"),
        "omit-owner-local-report": ("Invocation", "fun o => parent o ∨ reported o", "fun o => parent o"),
        "admit-nonempty-return": ("Invocation", "(tokens == 0) && selectorReady", "selectorReady"),
        "reverse-identity-selection": ("Selection", "if left = right then equal else different", "if left = right then different else equal"),
        "drop-capture-reachability": ("Capture", "Reach.edge (Reach.root live) capture", "Reach.root live"),
        "ignore-bulk-own-binding": ("Capture", "bulkCurrent && readerCurrent && queryCurrent && servicesCurrent", "readerCurrent && queryCurrent && servicesCurrent"),
        "ignore-capture-query-binding": ("Capture", "\n  readerCurrent && queryCurrent && servicesCurrent", "\n  readerCurrent && servicesCurrent"),
        "reverse-branch-choice": ("Branches", "| .yes => yes | .no => no | .other => other", "| .yes => no | .no => yes | .other => other"),
        "unrooted-local-hint": ("LocalReuse", "(fun o => roots o ∨ o = value) (fun o => hints o ∨ o = value)", "roots (fun o => hints o ∨ o = value)"),
        "cache-false-selector": ("SymbolNames", "(agrees : ∀ n, cache n = canonical n)", "(agrees : ∀ n, cache n = cache n)"),
        "drop-uncovered-locals": ("Invocation", "\n  fun o => parent o ∨ locals o", "\n  fun o => parent o"),
        "ignore-entry-context": ("Admission", "(f.method == f.expectedMethod) && (f.context == f.expectedContext)\n\n/-- A declined", "(f.method == f.expectedMethod) && true\n\n/-- A declined"),
        "wrong-release-order": ("GC", "callerTokens.reverse.foldl release st", "callerTokens.foldl release st"),
        "lost-index-mark": ("GC", "indexed := if st.indexed (index o) then st.indexed else fun i => if i = index o then true else st.indexed i", "indexed := st.indexed"),
        "wrong-memo-key": ("Memo", "if key = k then some v else lookup rest key", "some v"),
        "truncated-borrow": ("Borrow", "bytes.map id", "[]"),
        "ignore-observation": ("Semantics", "(reference a).observations = (optimized b).observations", "True"),
        "ignore-permission": ("Semantics", "(∀ k, k ∈ c.permissions → w.grants k = true)", "True"),
        "free-owner-batch": ("Semantics", "OwnerSchedule (List.replicate (n+1) owner ++ tail) (cost+1)", "OwnerSchedule (List.replicate (n+1) owner ++ tail) cost"),
    }
    for name, (module, before, after) in extra_mutations.items():
        text = ((SEMANTICS if module in {"Semantics", "SemanticsExamples", "Admission", "IndexedSnapshot", "Relational", "SlotHints", "JsonStrings", "JsonDispatch", "JsonNumbers", "RangeHandlers", "LogMessages", "CollectionSearch", "StringOrder", "PrivateScratch", "SlotReflection", "ModuleRegistry", "GlobalsBinding", "PrivateIndex"} else FORMAL) / f"lean/{module}.lean").read_text()
        if text.count(before) != 1: raise RuntimeError(f"mutation {name} is stale")
        changed = build / f"Broken_{name.replace('-', '_')}.lean"
        changed.write_text(text.replace(before, after))
        result, _ = run([str(lean), str(changed)], out / f"lean-mutation-{name}.log", env=env)
        if result.returncode == 0 or "error:" not in result.stdout:
            raise RuntimeError(f"Lean failed to reject mutation {name}")
        print(f"Lean {name}: rejected", flush=True)
    for name, (before, after) in mutations.items():
        if source.count(before) != 1:
            raise RuntimeError(f"mutation {name} is stale")
        changed = build / f"Broken_{name.replace('-', '_')}.lean"
        changed.write_text(source.replace(before, after))
        result, _ = run([str(lean), str(changed)], out / f"lean-mutation-{name}.log", env=env)
        if result.returncode == 0 or "error:" not in result.stdout or not any(
                message in result.stdout for message in ["unsolved goals", "failed", "Type mismatch"]):
            raise RuntimeError(f"Lean failed to reject mutation {name}")
        print(f"Lean {name}: rejected", flush=True)
    print(f"Lean: {len(names)} theorems checked; only standard kernel axioms", flush=True)
    return {"theorems": names, "allowed_axioms": sorted(ALLOWED_AXIOMS),
            "rejected_mutations": list(mutations) + list(extra_mutations)}


def source_hashes():
    files = sorted(p for root in [FORMAL,SEMANTICS] for p in root.rglob("*") if p.is_file() and ".lake" not in p.parts)
    files.extend([Path(__file__).resolve(), ROOT / "mise.toml"])
    return {str(p.relative_to(ROOT)): digest(p) for p in files}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fetch-tools", action="store_true", help="download pinned, checksum-verified tools into ignored cache")
    parser.add_argument("--tla-jar", help="explicit pinned jar")
    parser.add_argument("--lean", help="explicit Lean 4.24.0 executable")
    parser.add_argument("--java", help="Java executable")
    args = parser.parse_args()
    os.nice(5)
    jar, lean, java, version = tools(args)
    results = CACHE / "results"
    results.mkdir(parents=True, exist_ok=True)
    out = Path(tempfile.mkdtemp(prefix="rooting-", dir=results))
    start = time.monotonic()
    before = source_hashes()
    try:
        tla = check_tla(jar, java, out)
        proof = check_lean(lean, out)
        after = source_hashes()
        if before != after:
            raise RuntimeError("formal inputs changed during verification; rerun on stable sources")
        summary = {"scope": "Abstract rooting and costed behavioral semantics; no production compiler verification",
                   "lean_version": version, "tla_jar_sha256": digest(jar),
                   "lean_binary_sha256": digest(lean), "seconds": round(time.monotonic()-start, 3),
                   "source_sha256": after,
                   "tla": tla, "lean": proof}
        (out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
        print(f"Rooting verification passed: {out / 'summary.json'}")
    except (RuntimeError, subprocess.TimeoutExpired, OSError) as error:
        (out / "failure.txt").write_text(str(error) + "\n")
        raise


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, subprocess.SubprocessError, OSError) as error:
        print(f"rooting verification failed: {error}", file=sys.stderr)
        sys.exit(1)
