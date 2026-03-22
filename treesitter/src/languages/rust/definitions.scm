;; Top-level Rust definitions.
;; Capture format: @definition.<kind>
;; The kind becomes the DefinitionKind in the index.

(function_item name: (identifier) @definition.function)
(struct_item name: (type_identifier) @definition.struct)
(enum_item name: (type_identifier) @definition.enum)
(trait_item name: (type_identifier) @definition.trait)

;; impl Type { ... } — capture the type being implemented
(impl_item type: (type_identifier) @definition.impl)

(const_item name: (identifier) @definition.const)
(type_item name: (type_identifier) @definition.type)
(mod_item name: (identifier) @definition.module)
