# FINDING-0002: Long user instructions changed provider and tool read authority

The reviewed pre-freeze implementation expanded long user instructions for provider/tool authority while deriving file preview and initialization policy from only the short reference placeholder. Crossing the inline-size threshold could therefore change which workspace reads were authorized.

The final implementation uses one authenticated user-instruction expansion for provider capture, file preview, artifact initialization, and legacy initialization. It expands only selected user messages or accepted proposals and keeps ordinary attached data inert. Equal inline/reference and legacy initialization regressions pass.
