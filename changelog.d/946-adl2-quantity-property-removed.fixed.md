- The ADL 2 DV_QUANTITY templates no longer constrain `property`, which the
  released RM DV_QUANTITY does not have, so a server that refuses them by
  AOM2 VCARM is no longer marked down. The units and the units-magnitude
  templates become `cnf_adl2_dv_quantity_units` and
  `cnf_adl2_dv_quantity_units_mag`, the Celsius interval template becomes
  `cnf_adl2_dv_interval_quantity_celsius`, and their payloads drop the
  `property` member. The four cases that asserted a property match or a
  property refusal are removed, and the tuple refusals name report #946.
  Every ADL 2 accepted commit names report #945.
