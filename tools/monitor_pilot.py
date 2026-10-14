#!/usr/bin/env python3
"""Track actual ComfyUI queue activity, NVIDIA GPU peak VRAM and host RAM.
Start BEFORE pressing Queue Prompt. Samples include total GPU use (not process-only).
Works with standard-library Python 3 on Windows; also works from WSL if nvidia-smi exists.
"""
import argparse
import csv
import datetime as dt
import json
import math
import os
import re
import shutil
import statistics
import subprocess
import sys
import time
import urllib.request
from pathlib import Path


def find_smi():
    value = shutil.which("nvidia-smi")
    if value:
        return value
    for prefix in (os.environ.get("ProgramFiles", ""), os.environ.get("ProgramW6432", "")):
        p = Path(prefix) / "NVIDIA Corporation" / "NVSMI" / "nvidia-smi.exe"
        if prefix and p.is_file():
            return str(p)
    return None


def number(value):
    result = re.search(r"[-+]?\d+(?:\.\d+)?", str(value))
    return float(result.group()) if result else None


def gpu_snapshot(command, selected_index=None):
    if not command:
        return {"warning": "nvidia-smi not found"}
    cmd = [command, '--query-gpu=index,name,memory.total,memory.used,utilization.gpu,temperature.gpu,power.draw', '--format=csv,noheader,nounits']
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=8, check=False)
        if p.returncode != 0:
            return {"warning": p.stderr.strip()[:200] or f"nvidia-smi return code {p.returncode}"}
        rows = list(csv.reader(p.stdout.splitlines()))
        choices = []
        for row in rows:
            if len(row) < 7:
                continue
            idx = number(row[0])
            choices.append({"gpu_index": int(idx) if idx is not None else 0,
                            "gpu_name": row[1].strip(), "vram_total_mib": number(row[2]),
                            "vram_used_mib": number(row[3]), "gpu_util_percent": number(row[4]),
                            "temperature_c": number(row[5]), "power_w": number(row[6])})
        if not choices:
            return {"warning": "nvidia-smi returned no GPU rows"}
        if selected_index is None:
            found = next((r for r in choices if "5080" in r["gpu_name"]), choices[0])
        else:
            found = next((r for r in choices if r["gpu_index"] == selected_index), None)
            if found is None:
                return {"warning": f"GPU index {selected_index} not found"}
        return found
    except (subprocess.SubprocessError, OSError) as ex:
        return {"warning": str(ex)[:200]}


def windows_ram_mib():
    if sys.platform != 'win32':
        try:
            data = {}
            for line in Path('/proc/meminfo').read_text().splitlines():
                if ':' in line:
                    k,v=line.split(':',1);data[k] = float(v.strip().split()[0]) / 1024
            return data['MemTotal'], data['MemTotal']-data['MemAvailable']
        except (OSError, KeyError):
            return None, None
    import ctypes
    from ctypes import wintypes
    class MEMORYSTATUSEX(ctypes.Structure):
        _fields_ = [('dwLength', wintypes.DWORD), ('dwMemoryLoad', wintypes.DWORD),
                    ('ullTotalPhys', ctypes.c_ulonglong), ('ullAvailPhys', ctypes.c_ulonglong),
                    ('ullTotalPageFile', ctypes.c_ulonglong), ('ullAvailPageFile', ctypes.c_ulonglong),
                    ('ullTotalVirtual', ctypes.c_ulonglong), ('ullAvailVirtual', ctypes.c_ulonglong),
                    ('ullAvailExtendedVirtual', ctypes.c_ulonglong)]
    status = MEMORYSTATUSEX()
    status.dwLength = ctypes.sizeof(status)
    if ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(status)):
        return status.ullTotalPhys/(1024**2), (status.ullTotalPhys-status.ullAvailPhys)/(1024**2)
    return None, None


def queue_state(base, timeout):
    try:
        req = urllib.request.Request(base.rstrip('/') + '/queue', headers={'Accept':'application/json'})
        with urllib.request.urlopen(req, timeout=timeout) as r:
            result=json.load(r)
        running=len(result.get('queue_running',[]))
        pending=len(result.get('queue_pending',[]))
        return running, pending, None
    except Exception as ex:
        return None, None, str(ex)


def main():
    ap=argparse.ArgumentParser(description=__doc__)
    root=Path(__file__).resolve().parent.parent
    ap.add_argument('--url',default='http://127.0.0.1:8188')
    ap.add_argument('--seconds',type=int,default=1800)
    ap.add_argument('--interval',type=float,default=2.0)
    ap.add_argument('--gpu-index',type=int,default=None)
    ap.add_argument('--station',default='unspecified',help='Workstation label: e.g. 4070ti-super or 5080')
    ap.add_argument('--out',type=Path,default=root/'reports')
    ap.add_argument('--no-queue',action='store_true',help='Sample for full period without waiting for ComfyUI queue')
    args=ap.parse_args()
    if not re.fullmatch(r'[A-Za-z0-9_-]{1,48}',args.station):
        ap.error('station must contain 1-48 ASCII letters, digits, underscores, or hyphens')
    if args.seconds < 1 or args.interval < 0.5:
        ap.error('seconds must be >=1; interval must be >= 0.5')
    args.out.mkdir(parents=True,exist_ok=True)
    prefix=args.out/('GPU_Pilot_'+dt.datetime.now().strftime('%Y%m%d_%H%M%S'))
    csvfile=Path(str(prefix)+'.csv');jsonfile=Path(str(prefix)+'.json')
    smi=find_smi()
    if not args.no_queue:
        running,pending,err=queue_state(args.url,5)
        if err is not None:
            print('ERROR: ComfyUI queue API is inaccessible:',err,file=sys.stderr)
            print('Check the -ComfyUrl address and Docker published port before measuring.',file=sys.stderr)
            return 2
    fields=['station','timestamp_local','elapsed_seconds','comfy_running','comfy_pending','queue_error','gpu_index',
            'gpu_name','vram_total_mib','vram_used_mib','gpu_util_percent','temperature_c',
            'power_w','host_ram_total_mib','host_ram_used_mib','warning']
    initial=time.monotonic();seen=False;firstbusy=None;lastbusy=None;idle_since=None;rows=[]
    print('Recording:',csvfile)
    print('Click Queue Prompt in ComfyUI AFTER this monitor starts. Ctrl+C stops safely.')
    if not smi: print('WARNING: nvidia-smi was not found, GPU metrics cannot be measured.')
    try:
        with csvfile.open('w',encoding='utf-8',newline='') as f:
            writer=csv.DictWriter(f,fieldnames=fields);writer.writeheader()
            last_print=-999
            while time.monotonic()-initial < args.seconds:
                start=time.monotonic();elapsed=round(start-initial,2)
                if args.no_queue: running=pending=None;err=None
                else: running,pending,err=queue_state(args.url,4)
                active = running is not None and (running+pending)>0
                if active:
                    seen=True;idle_since=None;lastbusy=elapsed
                    if firstbusy is None: firstbusy=elapsed;print('ComfyUI queue became active at',elapsed,'seconds')
                elif seen and running is not None:
                    if idle_since is None: idle_since=elapsed
                gpu=gpu_snapshot(smi,args.gpu_index)
                total_ram,used_ram=windows_ram_mib()
                row={'station':args.station,
                     'timestamp_local':dt.datetime.now().astimezone().isoformat(timespec='seconds'),
                     'elapsed_seconds':elapsed,'comfy_running':running,'comfy_pending':pending,
                     'queue_error':err,'host_ram_total_mib':total_ram,'host_ram_used_mib':used_ram}
                row.update(gpu)
                writer.writerow(row);f.flush();rows.append(row)
                if elapsed-last_print >= 20:
                    print(f"t={elapsed:.0f}s queue={running}/{pending} VRAM={gpu.get('vram_used_mib')} MiB util={gpu.get('gpu_util_percent')}%")
                    last_print=elapsed
                if not args.no_queue and idle_since is not None and elapsed-idle_since >= 8:
                    print('ComfyUI queue has been idle for 8 seconds; ending measurement.')
                    break
                time.sleep(max(0.0,args.interval-(time.monotonic()-start)))
    except KeyboardInterrupt:
        print('Stopped by user; partial samples saved.')
    def maxnum(key):
        v=[float(r[key]) for r in rows if isinstance(r.get(key),(float,int)) and math.isfinite(float(r[key]))]
        return max(v) if v else None
    def avgnum(key):
        v=[float(r[key]) for r in rows if isinstance(r.get(key),(float,int)) and math.isfinite(float(r[key]))]
        return round(statistics.mean(v),2) if v else None
    report={'station':args.station,
            'source':'real nvidia-smi / real local ComfyUI queue samples',
            'created_local':dt.datetime.now().astimezone().isoformat(), 'comfy_url':args.url,
            'gpu_command':str(smi) if smi else None,'queue_observed':seen,
            'queue_first_active_sec':firstbusy,'queue_last_active_sec':lastbusy,
            'busy_window_lower_bound_sec':round(lastbusy-firstbusy,2) if lastbusy is not None else None,
            'wall_monitor_seconds':round(time.monotonic()-initial,2),'samples':len(rows),
            'peak_total_gpu_vram_used_mib':maxnum('vram_used_mib'),
            'avg_gpu_util_percent':avgnum('gpu_util_percent'),
            'peak_gpu_util_percent':maxnum('gpu_util_percent'),
            'peak_host_ram_used_mib':maxnum('host_ram_used_mib'),
            'peak_gpu_temp_c':maxnum('temperature_c'),
            'gpu_name':next((r['gpu_name'] for r in rows if r.get('gpu_name')),None),
            'caveats':['VRAM includes all processes on selected GPU, not only ComfyUI.',
                       'Queue timing is sampled, not an exact ComfyUI execution trace.',
                       'Successful 3D output must be checked separately.']}
    jsonfile.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print('REPORT:',jsonfile)
    print(json.dumps({k:v for k,v in report.items() if k in ('queue_observed','samples','peak_total_gpu_vram_used_mib','avg_gpu_util_percent','wall_monitor_seconds')},indent=2))

if __name__=='__main__': sys.exit(main())
