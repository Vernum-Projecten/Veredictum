- The eight CONTRIBUTION attestation ids in `catalogue/id-map.yaml` stay
  uncarried and now name upstream report #751: the released commit DTO
  requires `lifecycle_state` and `data` on every member and names no
  existing version to attest, so it cannot express the RM's attestation of
  an existing version. The report's patch proposes the member shape.
