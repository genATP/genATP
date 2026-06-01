$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = ".\target\release\genatp.exe"
$psi.Arguments = "serve --stdio"
$psi.RedirectStandardInput = $true
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.UseShellExecute = $false
$psi.CreateNoWindow = $true

$proc = New-Object System.Diagnostics.Process
$proc.StartInfo = $psi
$null = $proc.Start()

$stdin = $proc.StandardInput
$stdout = $proc.StandardOutput

# 1) initialize
$stdin.WriteLine('{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"manual-test","version":"0.1.0"}}}')
$stdin.Flush()
$initResp = $stdout.ReadLine()
"INIT RESPONSE: $initResp"

# 2) tools/list
$stdin.WriteLine('{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}')
$stdin.Flush()
$toolsResp = $stdout.ReadLine()
"TOOLS RESPONSE: $toolsResp"

$proc.Kill()
