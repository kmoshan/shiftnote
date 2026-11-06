param([int]$Tail = 0)

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

$root = [System.Windows.Automation.AutomationElement]::RootElement
$appPid = (Get-Process shiftnote).Id

$procCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ProcessIdProperty, $appPid)
$window = $root.FindFirst([System.Windows.Automation.TreeScope]::Children, $procCond)
if (-not $window) { Write-Output "NO_WINDOW"; exit 2 }

$textCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
    [System.Windows.Automation.ControlType]::Text)
$texts = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants, $textCond)

$names = @()
foreach ($t in $texts) {
    $n = $t.Current.Name
    if ($n -and $n.Trim().Length -gt 0) { $names += $n }
}

if ($Tail -gt 0 -and $names.Count -gt $Tail) {
    $names = $names[($names.Count - $Tail)..($names.Count - 1)]
}

Write-Output ("COUNT: " + $names.Count)
$names | ForEach-Object { Write-Output ("  - " + $_) }
