param(
    [string]$Year = '',
    [string]$Month = '',
    [string]$Day = ''
)

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

$root = [System.Windows.Automation.AutomationElement]::RootElement
$appPid = (Get-Process shiftnote).Id
$procCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ProcessIdProperty, $appPid)
$spinnerCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
    [System.Windows.Automation.ControlType]::Spinner)

$spinners = $root.FindAll(
    [System.Windows.Automation.TreeScope]::Descendants,
    (New-Object System.Windows.Automation.AndCondition(@($procCond, $spinnerCond))))

Write-Output ("spinner count = " + $spinners.Count)
if ($spinners.Count -lt 3) { exit 2 }

$values = @($Year, $Month, $Day)
for ($i = 0; $i -lt 3; $i++) {
    if (-not $values[$i]) { continue }
    $pattern = $null
    if ($spinners[$i].TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) {
        $pattern.SetValue($values[$i])
        Start-Sleep -Milliseconds 300
        Write-Output ("set segment " + $i + " = " + $values[$i] + " (now " + $pattern.Current.Value + ")")
    } else {
        Write-Output ("no ValuePattern on segment " + $i)
    }
}

Start-Sleep -Milliseconds 700
$textCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
    [System.Windows.Automation.ControlType]::Text)
$texts = $root.FindAll(
    [System.Windows.Automation.TreeScope]::Descendants,
    (New-Object System.Windows.Automation.AndCondition(@($procCond, $textCond))))
$names = @()
foreach ($t in $texts) { $names += $t.Current.Name }
Write-Output ("TEXTS: " + ($names -join " / "))
