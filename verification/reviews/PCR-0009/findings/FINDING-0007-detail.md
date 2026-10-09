# FINDING-0007: Structured review required a complete diff within fixed collection caps

Severity: high

Blocking: true

Disposition: fixed in the final reviewed candidate.

The review endpoint parsed and embedded the complete diff before serving any page. `WorkbenchReviewPage` rejected valid diffs above 512 files, 4,096 hunks, or 32,768 lines, and an intermediate repair silently placed an incomplete one-file preview in the legacy response without a completeness marker. The final repair negotiates a distinct summary/page feature, preserves the legacy complete-or-error contract, and binds bounded diff pages and raw ranges to the exact candidate, revision, and whole-diff digest.
