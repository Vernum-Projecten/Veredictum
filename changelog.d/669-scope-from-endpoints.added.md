- The console's Scope step starts from the endpoints the server listed in its
  System OPTIONS document: a capability whose top-level resources the server
  listed starts checked, and one addressing a resource it did not list starts
  unchecked, where the operator can still check it. Each capability's
  resources are derived from the operations its workflows call through the
  released OpenAPI sources, and the engine's catalogue listing carries them
  (`ListedDocument::resources`, `Listing::capability_resources`).
