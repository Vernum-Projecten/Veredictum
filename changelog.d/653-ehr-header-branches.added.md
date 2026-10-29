- The Arazzo catalogue covers the EHR create header branches (#653, #659):
  `Prefer: return=minimal` on both creates, `Prefer: return=identifier` on
  the create with a client-supplied `ehr_id`, and `Accept: application/xml`
  on both creates under the `ehr-xml-read` option. Four new options,
  `ehr-status-flat-write`, `ehr-status-structured-write`,
  `ehr-status-flat-read` and `ehr-status-structured-read`, hold a service
  that declines a Simplified Format on the EHR and EHR_STATUS routes to
  `415` and `406`. The release defines no EHR_STATUS form in either format
  (upstream report #352), and no sentence requires a service to accept the
  XML root an EHR_STATUS has (#759), so those branches stay surface gaps
  that name the report.
