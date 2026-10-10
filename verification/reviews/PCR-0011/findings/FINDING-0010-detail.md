# FINDING-0010: PowerShell 5.1 coerced a null File.Replace backup path to an empty string

Frozen predecessor 51274d823 supplied `$null` for the optional backup path of .NET File.Replace. On hosted PowerShell 5.1 that argument bound as an empty path, so three deferred Windows updater cases failed instead of atomically publishing their outcomes.

The final candidate passes `[System.Management.Automation.Language.NullString]::Value`, retaining atomic replacement and Flush(true) while supplying an actual null reference. Cross-target strict checking and the complete local launcher suite pass; the exact final Windows H0 run passed all 16 native tests and the complete conformance case.
