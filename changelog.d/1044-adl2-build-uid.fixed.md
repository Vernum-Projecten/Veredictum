- Every ADL 2 template the catalogue uploads carries a `build_uid` in its
  header, which the AOM2 class table makes mandatory, so a server that is
  right to refuse a template without one no longer fails a case for it. The
  two refusal payloads that break off inside the header are unchanged.
