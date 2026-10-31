- `veredictum emit-schemas` exits with status 2 and names the schema file when
  a schema cannot be serialized, where it wrote an empty file before, and a
  vocabulary token that is not a string stops the emission instead of
  publishing `null` or an empty name.
