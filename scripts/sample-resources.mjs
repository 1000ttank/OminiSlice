import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { writeFile, mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';
const execute = promisify(execFile);
const root = Number(process.argv[2]);
const seconds = Number(process.argv[3] || 30);
if (!Number.isInteger(root) || root <= 0 || !Number.isInteger(seconds) || seconds < 5 || seconds > 300) throw new Error('Usage: node scripts/sample-resources.mjs <owned-app-pid> [seconds 5..300]');
const output = resolve('.workflow/native-evidence');
await mkdir(output, { recursive: true });
const samples = [];
const started = Date.now();
while (Date.now() - started < seconds * 1000) {
  const script = `$taskAll=Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId; $taskIds=[System.Collections.Generic.HashSet[int]]::new(); [void]$taskIds.Add(${root}); do { $taskChanged=$false; foreach($taskItem in $taskAll) { if($taskIds.Contains([int]$taskItem.ParentProcessId) -and $taskIds.Add([int]$taskItem.ProcessId)){$taskChanged=$true} } } while($taskChanged); @(Get-Process -Id @($taskIds) -ErrorAction SilentlyContinue | Select-Object Id,ProcessName,CPU,WorkingSet64,PrivateMemorySize64) | ConvertTo-Json -Compress`;
  const { stdout } = await execute('powershell.exe', ['-NoProfile', '-Command', script], { windowsHide: true, timeout: 10000 });
  const processes = JSON.parse(stdout);
  if (!processes.some(item => item.Id === root && item.ProcessName === 'omnislice')) throw new Error('Target is not the running OmniSlice application');
  samples.push({ time: Date.now(), processes });
  await new Promise(resolve => setTimeout(resolve, 1000));
}
const result = { root, seconds, samples, notes: ['包含应用及父子关系可追溯的 WebView2 进程', 'WorkingSet 合计含共享页重复计数，PrivateMemorySize 为提交内存', '短时开发构建基线，不能代表安装包体积或长期稳定性', 'GPU、独立网络流量、30 分钟稳定性未测量'] };
await writeFile(resolve(output, `resources-${root}-${started}.json`), JSON.stringify(result, null, 2));
console.log(JSON.stringify({ samples: samples.length, peakWorkingSetMiB: Math.max(...samples.map(sample => sample.processes.reduce((sum, process) => sum + process.WorkingSet64, 0))) / 1048576, peakPrivateMiB: Math.max(...samples.map(sample => sample.processes.reduce((sum, process) => sum + process.PrivateMemorySize64, 0))) / 1048576 }));
