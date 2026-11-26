"""Times folio-pdf runs: wall time, CPU time (user + kernel, Windows GetProcessTimes), peak
working set (from the report), stage timings and file size; repeated runs with medians.

Usage: python perf.py <runs> <out.json> <cases.json>   (cases: [{"name": ..., "cmd": [exe, args...]}])
The first run of each case is "cold" (first process after the build / other work); the
rest are "warm" (OS file cache filled).
"""
import sys, json, subprocess, time, statistics, ctypes, os
from ctypes import wintypes
sys.stdout.reconfigure(encoding='utf-8')


def cpu_ms(handle):
    k = ctypes.windll.kernel32
    c, e, kt, ut = (wintypes.FILETIME() for _ in range(4))
    if not k.GetProcessTimes(wintypes.HANDLE(handle), ctypes.byref(c), ctypes.byref(e), ctypes.byref(kt), ctypes.byref(ut)):
        return None
    v = lambda f: (f.dwHighDateTime << 32 | f.dwLowDateTime) / 10_000
    return v(kt) + v(ut)


def run_case(name, cmd, runs):
    out_pdf = cmd[2]
    rows = []
    for i in range(runs):
        t = time.perf_counter()
        p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        p.communicate()
        wall = (time.perf_counter() - t) * 1000
        cpu = cpu_ms(p._handle)
        rep = json.load(open(out_pdf + '.report.json', encoding='utf-8'))
        rows.append({'run': i + 1, 'exit': p.returncode, 'wallMs': round(wall, 1), 'cpuMs': round(cpu, 1) if cpu else None,
                     'peakMB': round(rep['peakWorkingSetBytes'] / 2**20, 1), 'bytes': os.path.getsize(out_pdf),
                     'stages': {k: round(v, 1) for k, v in rep['stages'].items()},
                     'exportPhases': {k: round(v, 1) for k, v in rep['timings_ms'].items()}})
    warm = rows[1:] or rows
    med = lambda k: statistics.median(r[k] for r in warm)
    summary = {'case': name, 'cold': rows[0], 'warmMedian': {k: med(k) for k in ('wallMs', 'cpuMs', 'peakMB')},
               'warmMedianExportMs': statistics.median(r['stages']['exportMs'] for r in warm),
               'warmRange': {'wallMs': [min(r['wallMs'] for r in warm), max(r['wallMs'] for r in warm)]},
               'bytes': rows[-1]['bytes'], 'runs': rows}
    return summary


def main():
    runs, out = int(sys.argv[1]), sys.argv[2]
    cases = json.load(open(sys.argv[3], encoding='utf-8'))
    results = []
    for c in cases:
        name, cmd = c['name'], c['cmd']
        s = run_case(name, cmd, runs)
        results.append(s)
        print(name, 'cold', s['cold']['wallMs'], 'ms; warm median', s['warmMedian'], 'export', s['warmMedianExportMs'], 'bytes', s['bytes'])
    json.dump(results, open(out, 'w', encoding='utf-8'), indent=1, ensure_ascii=False)


main()
