$ErrorActionPreference = "SilentlyContinue"
$ServerUrl = if ($env:IRIS_SERVER_URL) { $env:IRIS_SERVER_URL.TrimEnd('/') } else { "https://iris.zacharie.org" }
$TokenLimit = if ($env:ANTIGRAVITY_TOKEN_LIMIT) { [int64]$env:ANTIGRAVITY_TOKEN_LIMIT } else { 2000000 }
$ModelName = if ($env:ANTIGRAVITY_MODEL) { $env:ANTIGRAVITY_MODEL } else { "Gemini 4 Hydrogen" }
$LastActionId = ""
$Tick = 0

Write-Host "Iris PC Agent (PowerShell Bridge) started -> $ServerUrl"

while ($true) {
    if (($Tick % 3) -eq 0) {
        try {
            $cpu = [int](Get-CimInstance Win32_Processor | Measure-Object -Property LoadPercentage -Average).Average
            $os = Get-CimInstance Win32_OperatingSystem
            $ramTotalMb = [int64]([math]::Round($os.TotalVisibleMemorySize / 1024))
            $ramFreeMb = [int64]([math]::Round($os.FreePhysicalMemory / 1024))
            $ramUsedMb = $ramTotalMb - $ramFreeMb
            $ramPct = if ($ramTotalMb -gt 0) { [int]([math]::Round(($ramUsedMb * 100.0) / $ramTotalMb)) } else { 0 }

            $gpuName = ""
            $gpuPct = 0
            $gpuMemUsed = 0
            $gpuMemTotal = 0
            $gpuMemPct = 0

            $smi = & nvidia-smi --query-gpu=name,utilization.gpu,memory.used,memory.total --format=csv,noheader,nounits 2>$null
            if ($smi) {
                $parts = ($smi | Select-Object -First 1).Split(',') | ForEach-Object { $_.Trim() }
                if ($parts.Count -ge 4) {
                    $gpuName = $parts[0]
                    $gpuPct = [int]$parts[1]
                    $gpuMemUsed = [int64]$parts[2]
                    $gpuMemTotal = [int64]$parts[3]
                    if ($gpuMemTotal -gt 0) {
                        $gpuMemPct = [int]([math]::Round(($gpuMemUsed * 100.0) / $gpuMemTotal))
                    }
                }
            }

            $activeMeeting = $null
            if (Get-Process -Name "ms-teams","Teams" -ErrorAction SilentlyContinue) {
                $activeMeeting = "Teams"
            } elseif (Get-Process -Name "Zoom" -ErrorAction SilentlyContinue) {
                $activeMeeting = "Zoom"
            }

            $uptimeSecs = [int64]((Get-Date) - $os.LastBootUpTime).TotalSeconds
            $nowMs = [int64]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())

            $payload = @{
                hostname = $env:COMPUTERNAME
                cpuPercent = $cpu
                ramUsedMb = $ramUsedMb
                ramTotalMb = $ramTotalMb
                ramPercent = $ramPct
                gpuName = $gpuName
                gpuPercent = $gpuPct
                gpuMemUsedMb = $gpuMemUsed
                gpuMemTotalMb = $gpuMemTotal
                gpuMemPercent = $gpuMemPct
                uptimeSecs = $uptimeSecs
                activeMeeting = $activeMeeting
                antigravityTokensUsed = 320000
                antigravityTokensLimit = $TokenLimit
                antigravityModel = $ModelName
                updatedAtEpochMs = $nowMs
            } | ConvertTo-Json -Compress

            Invoke-RestMethod -Method Post -Uri "$ServerUrl/api/pc/telemetry" -ContentType "application/json" -Body $payload -TimeoutSec 5 | Out-Null
        } catch {}
    }

    try {
        $snap = Invoke-RestMethod -Method Get -Uri "$ServerUrl/api/state" -TimeoutSec 5
        $act = $snap.lastDeckAction
        if ($act -and $act.id) {
            if ($LastActionId -ne "" -and $LastActionId -ne $act.id) {
                $kind = $act.kind
                $target = $act.payload
                switch ($kind) {
                    "JOIN_TEAMS" { Start-Process (if ($target) { $target } else { "msteams://" }) }
                    "JOIN_ZOOM" { Start-Process (if ($target) { $target } else { "zoommtg://" }) }
                    "OPEN_URL" { if ($target) { Start-Process $target } }
                    "LAUNCH_APP" { if ($target) { Start-Process $target } }
                    "MEDIA_TOGGLE" { (New-Object -ComObject WScript.Shell).SendKeys([char]179) }
                    "LOCK_SCREEN" { Start-Process "rundll32.exe" -ArgumentList "user32.dll,LockWorkStation" }
                    "UPDATE_UNI_GET" {
                        Start-Process "powershell.exe" -ArgumentList "-NoExit","-NoProfile","-Command","winget upgrade --all --include-unknown --accept-package-agreements --accept-source-agreements"
                    }
                    "UPDATE_PROXMOX_VMS" {
                        Invoke-RestMethod -Method Post -Uri "$ServerUrl/api/infra/proxmox/update" -TimeoutSec 15 | Out-Null
                    }
                }
            }
            $LastActionId = $act.id
        } elseif ($LastActionId -eq "") {
            $LastActionId = "init"
        }
    } catch {}

    $Tick++
    Start-Sleep -Seconds 1
}
