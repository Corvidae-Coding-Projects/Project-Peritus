# FINDING-0001: Continuation admission could launch a different mode or input revision

The reviewed pre-freeze implementation separated continuation preparation from launch across an await. Another continuation could replace the chosen mode, and generic pending-input checks could admit a different revision after the requested input was edited, held, or withdrawn.

The final implementation validates the exact requested mode, original revision-one input, and accepted operation while retaining the record/control locks through persistence of the one-time launch marker. Interleaving tests cover competing prepared modes, revision changes, hold/withdraw, and replay. The final candidate contains the repaired bytes; no open instance remains in this transition.
