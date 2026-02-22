param(
    [Parameter(Mandatory = $false)]
    [string]$GateFile = "plans/scalability-benchmark-gates.yaml",

    [Parameter(Mandatory = $true)]
    [string]$MetricsFile,

    [Parameter(Mandatory = $false)]
    [ValidateSet("p0", "p1", "p2", "p3", "all")]
    [string]$Phase = "all",

    [Parameter(Mandatory = $false)]
    [switch]$PassThru
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Convert-Scalar {
    param([string]$Raw)

    $v = $Raw.Trim()
    if ($v.StartsWith('"') -and $v.EndsWith('"')) {
        return $v.Substring(1, $v.Length - 2)
    }
    if ($v -eq "true") { return $true }
    if ($v -eq "false") { return $false }

    $num = 0.0
    if ([double]::TryParse($v, [ref]$num)) {
        return $num
    }

    return $v
}

function Parse-GateYaml {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        throw "Gate file not found: $Path"
    }

    $lines = Get-Content -LiteralPath $Path
    $inPhases = $false
    $currentPhase = $null
    $inGates = $false
    $inPrerequisites = $false
    $currentItem = $null
    $currentTarget = $null

    $result = @{
        phases = @{}
        severity_map = @{
            critical = @()
            high = @()
            medium = @()
        }
    }

    foreach ($line in $lines) {
        if ($line -match '^phases:\s*$') {
            $inPhases = $true
            continue
        }

        if (-not $inPhases) {
            continue
        }

        if ($line -match '^gate_evaluation:\s*$') {
            if ($null -ne $currentItem -and $null -ne $currentPhase -and $null -ne $currentTarget) {
                $result.phases[$currentPhase][$currentTarget] += $currentItem
                $currentItem = $null
            }
            continue
        }

        if ($line -match '^  (p[0-9]+):\s*$') {
            if ($null -ne $currentItem -and $null -ne $currentPhase -and $null -ne $currentTarget) {
                $result.phases[$currentPhase][$currentTarget] += $currentItem
                $currentItem = $null
            }

            $currentPhase = $Matches[1]
            $result.phases[$currentPhase] = @{
                name = $currentPhase
                prerequisites = @()
                gates = @()
            }
            $inGates = $false
            $inPrerequisites = $false
            $currentTarget = $null
            continue
        }

        if ($line -match '^  severity_map:\s*$') {
            $currentPhase = $null
            $inGates = $false
            $inPrerequisites = $false
            $currentTarget = "severity_map"
            continue
        }

        if ($currentTarget -like "severity*") {
            if ($line -match '^    (critical|high|medium):\s*$') {
                $currentTarget = "severity_" + $Matches[1]
                continue
            }

            if ($line -match '^\s*-\s*"([^"]+)"\s*$') {
                switch ($currentTarget) {
                    "severity_critical" { $result.severity_map.critical += $Matches[1] }
                    "severity_high" { $result.severity_map.high += $Matches[1] }
                    "severity_medium" { $result.severity_map.medium += $Matches[1] }
                    default { }
                }
                continue
            }
        }

        if ($null -eq $currentPhase) {
            continue
        }

        if ($line -match '^    name:\s*"([^"]+)"\s*$') {
            $result.phases[$currentPhase].name = $Matches[1]
            continue
        }

        if ($line -match '^    prerequisites:\s*$') {
            if ($null -ne $currentItem -and $null -ne $currentTarget) {
                $result.phases[$currentPhase][$currentTarget] += $currentItem
                $currentItem = $null
            }
            $inPrerequisites = $true
            $inGates = $false
            $currentTarget = "prerequisites"
            continue
        }

        if ($line -match '^    gates:\s*$') {
            if ($null -ne $currentItem -and $null -ne $currentTarget) {
                $result.phases[$currentPhase][$currentTarget] += $currentItem
                $currentItem = $null
            }
            $inGates = $true
            $inPrerequisites = $false
            $currentTarget = "gates"
            continue
        }

        if ((-not $inGates) -and (-not $inPrerequisites)) {
            continue
        }

        if ($line -match '^      - id:\s*"([^"]+)"\s*$') {
            if ($null -ne $currentItem -and $null -ne $currentTarget) {
                $result.phases[$currentPhase][$currentTarget] += $currentItem
            }
            $currentItem = @{
                id = $Matches[1]
                metric = $null
                op = $null
                value = $null
            }
            continue
        }

        if ($null -eq $currentItem) {
            continue
        }

        if ($line -match '^        metric:\s*"([^"]+)"\s*$') {
            $currentItem.metric = $Matches[1]
            continue
        }

        if ($line -match '^        op:\s*"([^"]+)"\s*$') {
            $currentItem.op = $Matches[1]
            continue
        }

        if ($line -match '^        value:\s*(.+)\s*$') {
            $currentItem.value = Convert-Scalar -Raw $Matches[1]
            continue
        }
    }

    if ($null -ne $currentItem -and $null -ne $currentPhase -and $null -ne $currentTarget) {
        $result.phases[$currentPhase][$currentTarget] += $currentItem
    }

    return $result
}

function Get-MetricValue {
    param(
        [object]$Metrics,
        [string]$MetricName
    )

    if ($Metrics.PSObject.Properties.Name -contains $MetricName) {
        return $Metrics.$MetricName
    }

    if ($Metrics.PSObject.Properties.Name -contains "metrics") {
        $inner = $Metrics.metrics
        if ($inner -and ($inner.PSObject.Properties.Name -contains $MetricName)) {
            return $inner.$MetricName
        }
    }

    return $null
}

function Compare-Gate {
    param(
        [object]$Actual,
        [string]$Op,
        [object]$Expected
    )

    switch ($Op) {
        "==" { return $Actual -eq $Expected }
        "<"  { return [double]$Actual -lt [double]$Expected }
        "<=" { return [double]$Actual -le [double]$Expected }
        ">"  { return [double]$Actual -gt [double]$Expected }
        ">=" { return [double]$Actual -ge [double]$Expected }
        default { throw "Unsupported operator: $Op" }
    }
}

function Format-Value {
    param([object]$Value)
    if ($null -eq $Value) { return "<missing>" }
    return [string]$Value
}

function Get-FailureSeverity {
    param(
        [object[]]$Failures,
        [hashtable]$SeverityMap
    )

    $failedMetrics = @($Failures | ForEach-Object { $_.metric } | Where-Object { $_ -and $_ -ne "<blocked>" })

    foreach ($m in $failedMetrics) {
        if ($SeverityMap.critical -contains $m) {
            return "critical"
        }
    }
    foreach ($m in $failedMetrics) {
        if ($SeverityMap.high -contains $m) {
            return "high"
        }
    }
    foreach ($m in $failedMetrics) {
        if ($SeverityMap.medium -contains $m) {
            return "medium"
        }
    }

    return "unknown"
}

try {
    $gates = Parse-GateYaml -Path $GateFile

    if (-not (Test-Path -LiteralPath $MetricsFile)) {
        throw "Metrics file not found: $MetricsFile"
    }

    $metrics = Get-Content -LiteralPath $MetricsFile -Raw | ConvertFrom-Json

    $targetPhases = @()
    if ($Phase -eq "all") {
        $targetPhases = $gates.phases.Keys | Sort-Object
    } else {
        if (-not $gates.phases.ContainsKey($Phase)) {
            throw "Phase '$Phase' not found in gate file."
        }
        $targetPhases = @($Phase)
    }

    $results = @()
    foreach ($phaseKey in $targetPhases) {
        $phaseDef = $gates.phases[$phaseKey]
        $prereqFailed = $false

        foreach ($prereq in $phaseDef.prerequisites) {
            $actual = Get-MetricValue -Metrics $metrics -MetricName $prereq.metric
            $status = "FAIL"
            $reason = ""

            if ($null -eq $actual) {
                $reason = "missing metric"
            } else {
                $pass = Compare-Gate -Actual $actual -Op $prereq.op -Expected $prereq.value
                if ($pass) {
                    $status = "PASS"
                } else {
                    $reason = "prerequisite not met"
                }
            }

            if ($status -ne "PASS") {
                $prereqFailed = $true
            }

            $results += [pscustomobject]@{
                phase = $phaseKey
                phase_name = $phaseDef.name
                kind = "prerequisite"
                gate_id = $prereq.id
                metric = $prereq.metric
                actual = Format-Value -Value $actual
                op = $prereq.op
                expected = Format-Value -Value $prereq.value
                status = $status
                reason = $reason
            }
        }

        foreach ($gate in $phaseDef.gates) {
            if ($prereqFailed) {
                $results += [pscustomobject]@{
                    phase = $phaseKey
                    phase_name = $phaseDef.name
                    kind = "gate"
                    gate_id = $gate.id
                    metric = $gate.metric
                    actual = "<blocked>"
                    op = $gate.op
                    expected = Format-Value -Value $gate.value
                    status = "BLOCKED"
                    reason = "prerequisite failed"
                }
                continue
            }

            $actual = Get-MetricValue -Metrics $metrics -MetricName $gate.metric
            $status = "FAIL"
            $reason = ""

            if ($null -eq $actual) {
                $reason = "missing metric"
            } else {
                $pass = Compare-Gate -Actual $actual -Op $gate.op -Expected $gate.value
                if ($pass) {
                    $status = "PASS"
                    $reason = ""
                } else {
                    $reason = "threshold not met"
                }
            }

            $results += [pscustomobject]@{
                phase = $phaseKey
                phase_name = $phaseDef.name
                kind = "gate"
                gate_id = $gate.id
                metric = $gate.metric
                actual = Format-Value -Value $actual
                op = $gate.op
                expected = Format-Value -Value $gate.value
                status = $status
                reason = $reason
            }
        }
    }

    $results | Format-Table -AutoSize

    $total = @($results).Count
    $passed = @($results | Where-Object { $_.status -eq "PASS" }).Count
    $failed = @($results | Where-Object { $_.status -ne "PASS" }).Count

    Write-Host ""
    Write-Host "Summary: total=$total pass=$passed fail=$failed"

    if ($PassThru) {
        $results | ConvertTo-Json -Depth 6
    }

    if ($failed -gt 0) {
        $failures = @($results | Where-Object { $_.status -ne "PASS" })
        $severity = Get-FailureSeverity -Failures $failures -SeverityMap $gates.severity_map
        Write-Host "Failure severity: $severity"

        switch ($severity) {
            "critical" { exit 3 }
            "high" { exit 4 }
            "medium" { exit 5 }
            default { exit 2 }
        }
    }

    exit 0
}
catch {
    Write-Error $_
    exit 1
}
