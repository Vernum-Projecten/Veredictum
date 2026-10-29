- **Breaking:** the `veredictum run` subcommand is removed, with no alias.
  `veredictum test` drives the catalogue; a script that called `run` fails
  with a usage error until it calls `test`.
