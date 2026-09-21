"""Production Bootstrap admission with only Id and its actual support predecessors."""
from pathlib import Path
import os
import shutil
import subprocess
import sys
import tempfile

probe, lib, oracle = (Path(arg).resolve() for arg in sys.argv[1:4])
source = Path(__file__).resolve().parent
families = {
    "methodObservation": ("method_observation", "create destroy notify"),
    "observation": ("storage", "create destroy read set_active set_permit advance_epoch"),
    "deferredIdentity": ("storage", "create destroy read write"),
    "deferredIndex": ("storage", "destroy len capacity push read write swap_remove clear"),
    "dispatchCache": ("dispatch_cache", "create destroy lookup insert clear"),
    "guardVerdict": ("dispatch_cache", "create destroy read write"),
}
count = 0
with tempfile.TemporaryDirectory(prefix="birnam-support-admission-") as directory:
    work = Path(directory)
    proxy = work / "proxy.dylib"
    subprocess.run(["cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib",
                    str(source / "runtime-support-admission-native.c"), "-o", str(proxy)], check=True)

    def prefix(name, replaced=None):
        case = work / name
        case.mkdir()
        for stem in ["id", "bootstrap", "method_observation", "storage", "dispatch_cache"]:
            if stem != replaced:
                # Copies ensure the real loader observes this prefix, not a resolved symlink's siblings.
                shutil.copy2(lib / f"libbirnam_{stem}.dylib", case)
        return case

    def reject(case, status, env=None, detail=0):
        global count
        subprocess.run([str(probe), str(case), "reject", str(status), str(detail)], env=env, check=True)
        count += 1

    for library in ["method_observation", "storage", "dispatch_cache"]:
        for fault, status in [("missing", 2), ("entry", 3), ("version", 4), ("size", 4), ("initializer", 4)]:
            case = prefix(library + "-" + fault, library)
            if fault != "missing":
                c = work / "invalid.c"
                declaration = "int unrelated;" if fault == "entry" else (
                    f'const BirnamModule birnam_native_module_v1={{{2 if fault == "version" else 1},'
                    f'{8 if fault == "size" else "sizeof(BirnamModule)"},'
                    f'{"NULL" if fault == "initializer" else "unused"}}};')
                c.write_text('#include "' + str(source.parent.parent / "abi/fixtures/id-protocol-v1/id_protocol.h") + '"\n'
                             'static BirnamOutcome unused(const BirnamProtocol *id,const BirnamOperand *args,size_t n)'
                             '{(void)id;(void)args;(void)n;__builtin_trap();}\n' + declaration)
                subprocess.run(["cc", "-dynamiclib", str(c), "-o", str(case / f"libbirnam_{library}.dylib")], check=True)
            reject(case, status, detail=2 if fault == "version" else 1 if fault == "size" else 0)

    for request, (library, operations) in families.items():
        for fault in ["null", "kind", "failure", "version", "size", *operations.split()]:
            case = prefix(request + "-" + fault, library)
            shutil.copy2(proxy, case / f"libbirnam_{library}.dylib")
            receipt = work / "reached"
            receipt.unlink(missing_ok=True)
            env = dict(os.environ, BIRNAM_SUPPORT_ORIGINAL=str(lib / f"libbirnam_{library}.dylib"),
                       BIRNAM_SUPPORT_REQUEST=request, BIRNAM_SUPPORT_FAULT=fault,
                       BIRNAM_SUPPORT_REACHED=str(receipt))
            reject(case, 5 if fault == "failure" else 1, env)
            assert receipt.read_text() == request, (request, fault)

    # The same installed libraries also exercise every operation, with no Runtime present.
    subprocess.run([str(probe), str(prefix("valid")), str(oracle)], check=True)
    count += 1
print(f"Runtime support admission: {count} production-prefix cases passed", flush=True)
