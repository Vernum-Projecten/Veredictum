- The accepting arms of `ehr-xml-write`, `ehr-status-xml-write`,
  `directory-xml-write` and `party-xml-write` carry documents: 15 workflows
  that write the EHR creates, the EHR_STATUS update, the directory create and
  update, and the create and update of each PARTY kind as canonical XML and
  read the content back as JSON. The bodies are `items` elements typed with
  `xsi:type`, the one global LOCATABLE element the ITS-XML schemas publish,
  and the workflows rest on upstream report #759, which records that no
  released sentence names the root a service accepts.
- `scripts/checks/xml-write-bodies.sh` validates every XML body an accepting
  XML-write arm sends against the vendored ITS-XML schemas, with a seeded
  self-test, in the CI guards job and `scripts/checks/gates.sh`.
