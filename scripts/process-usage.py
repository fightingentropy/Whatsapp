#!/usr/bin/env python3
"""Sample resource counters for an existing macOS process, without changing it.

Example: python3 scripts/process-usage.py 1234 --seconds 15 > usage.json
CPU percentages use one full core as 100%. This does not measure energy use.
"""

import argparse
import ctypes
import json
import math
import sys
import time


class Usage(ctypes.Structure):
    # rusage_info_v2 from the macOS SDK's sys/resource.h.
    _fields_ = [("uuid", ctypes.c_uint8 * 16)] + [
        (name, ctypes.c_uint64)
        for name in (
            "user_time", "system_time", "pkg_idle_wkups", "interrupt_wkups",
            "pageins", "wired_size", "resident_size", "phys_footprint",
            "proc_start_abstime", "proc_exit_abstime", "child_user_time",
            "child_system_time", "child_pkg_idle_wkups", "child_interrupt_wkups",
            "child_pageins", "child_elapsed_abstime", "diskio_bytesread",
            "diskio_byteswritten",
        )
    ]


class Timebase(ctypes.Structure):
    _fields_ = [("numer", ctypes.c_uint32), ("denom", ctypes.c_uint32)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("pid", type=int)
    parser.add_argument("--seconds", type=float, default=15)
    parser.add_argument("--label", default="process")
    args = parser.parse_args()
    if (
        sys.platform != "darwin"
        or args.pid <= 0
        or not math.isfinite(args.seconds)
        or args.seconds <= 0
    ):
        parser.error("requires macOS, a positive PID and a positive duration")

    library = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
    library.proc_pid_rusage.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
    library.proc_pid_rusage.restype = ctypes.c_int
    system = ctypes.CDLL("/usr/lib/libSystem.B.dylib")
    system.mach_timebase_info.argtypes = [ctypes.POINTER(Timebase)]
    system.mach_timebase_info.restype = ctypes.c_int
    timebase = Timebase()
    if system.mach_timebase_info(ctypes.byref(timebase)) or not timebase.denom:
        raise RuntimeError("could not read the Mach clock conversion")

    samples = []
    deadline = time.monotonic() + args.seconds
    while True:
        usage = Usage()
        if library.proc_pid_rusage(args.pid, 2, ctypes.byref(usage)):
            raise OSError(ctypes.get_errno(), "proc_pid_rusage failed")
        samples.append({
            "monotonic": time.monotonic(),
            **{name: getattr(usage, name) for name, _ in Usage._fields_[1:]},
        })
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            break
        time.sleep(min(1, remaining))

    start, end = samples[0], samples[-1]
    if start["proc_start_abstime"] != end["proc_start_abstime"]:
        raise RuntimeError("the process changed during measurement")
    elapsed = end["monotonic"] - start["monotonic"]
    # These CPU counters are Mach ticks, not nanoseconds. On the measured M4 Pro
    # the conversion is 125/3; assuming nanoseconds understates CPU by 41.67x.
    cpu_ticks = sum(end[key] - start[key] for key in ("user_time", "system_time"))
    cpu_seconds = cpu_ticks * timebase.numer / timebase.denom / 1e9
    print(json.dumps({
        "label": args.label,
        "pid": args.pid,
        "duration_s": elapsed,
        "cpu_percent_one_core": 100 * cpu_seconds / elapsed,
        "mach_timebase": {"numer": timebase.numer, "denom": timebase.denom},
        "interrupt_wakeups_per_s": (end["interrupt_wkups"] - start["interrupt_wkups"]) / elapsed,
        "package_idle_wakeups_per_s": (end["pkg_idle_wkups"] - start["pkg_idle_wkups"]) / elapsed,
        "physical_footprint_mib": end["phys_footprint"] / 2**20,
        "resident_mib": end["resident_size"] / 2**20,
        "samples": samples,
    }, indent=2))


if __name__ == "__main__":
    main()
