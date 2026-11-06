param(
    [string]$Contains = '',
    [string]$ControlType = ''
)

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

$root = [System.Windows.Automation.AutomationElement]::RootElement
$conds = New-Object System.Collections.ArrayList

if ($ControlType) {
    [void]$conds.Add((New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::$ControlType)))
}

if ($conds.Count -eq 0) {
    $scope = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.Condition]::TrueCondition)
} else {
    $scope = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $conds[0])
}

$hits = @()
foreach ($element in $scope) {
    $name = $element.Current.Name
    if (-not $name) { continue }
    if ($Contains -and ($name.IndexOf($Contains, [System.StringComparison]::Ordinal) -lt 0)) { continue }
    $hits += ("[" + $element.Current.ControlType.ProgrammaticName + "] " + $name)
}

Write-Output ("MATCHES: " + $hits.Count)
$hits | Select-Object -First 20 | ForEach-Object { Write-Output ("  " + $_) }
