# FINDING-0025: A WebUI custody regression asserted an unpromised release ordering

The test tried the mutation mutex immediately after observing the durable result, although result publication occurs before the spawned task drops its guard. That produced a false failure without showing a product custody leak. The final regression waits with a fixed timeout on the actual lock and still requires the exact durable result and Git history, proving eventual custody release under the real ordering.
