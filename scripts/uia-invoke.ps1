param(
    [Parameter(Mandatory = $true)][string]$Name,
    [string]$ControlType = '',
    [int]$Index = 0,
    [int]$TimeoutSeconds = 10
)

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

$root = [System.Windows.Automation.AutomationElement]::RootElement
$deadline = (Get-Date).AddSeconds($TimeoutSeconds)

function Find-Named {
    param($name, $controlType)
    $conditions = New-Object System.Collections.ArrayList
    [void]$conditions.Add((New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::NameProperty, $name)))
    if ($controlType) {
        [void]$conditions.Add((New-Object System.Windows.Automation.PropertyCondition(
            [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
            [System.Windows.Automation.ControlType]::$controlType)))
    }
    $cond = if ($conditions.Count -eq 1) { $conditions[0] } else {
        New-Object System.Windows.Automation.AndCondition($conditions.ToArray())
    }
    return $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond)
}

$matches = $null
while ((Get-Date) -lt $deadline) {
    $matches = Find-Named -name $Name -controlType $ControlType
    if ($matches.Count -gt $Index) { break }
    Start-Sleep -Milliseconds 300
}

if (-not $matches -or $matches.Count -eq 0) {
    Write-Output "NOT_FOUND: $Name"
    exit 2
}

$element = $matches[$Index]
$pattern = $null
if ($element.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
    $pattern.Invoke()
    Write-Output "INVOKED: $Name (total=$($matches.Count))"
    exit 0
}
if ($element.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$pattern)) {
    $pattern.Select()
    Write-Output "SELECTED: $Name (total=$($matches.Count))"
    exit 0
}
if ($element.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern, [ref]$pattern)) {
    $pattern.Expand()
    Write-Output "EXPANDED: $Name (total=$($matches.Count))"
    exit 0
}

Write-Output "NO_PATTERN: $Name (total=$($matches.Count))"
exit 3
