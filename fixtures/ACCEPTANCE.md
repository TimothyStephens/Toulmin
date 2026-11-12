# Acceptance checklist (gate A manual exit criterion)

The owner completes this on one published paper of their own, using the
`tm` binary built from this repository, and signs it by date below. The
fixture under this directory is the worked example; it is not the paper.

- [ ] Paper named: ______________________________________________
- [ ] Every input table the results section rests on is registered
      (`tm file register`) with a title, a description, and a source
- [ ] Every registered table has a parse spec (`tm table spec`) and its
      key was checked for uniqueness
- [ ] Every claim in the paper's results section is entered
      (`tm claim add`), quantitative ones with steps and an assertion,
      qualitative ones citing a whole file, a page, or a panel
- [ ] Every verdict was reviewed in the static view (`tm view --out`),
      and each `fails` or `not checkable` was either corrected in the
      claim or confirmed as the paper's error
- [ ] `tm audit` on the finished project reports zero differences

Signed: ____________________    Date: ____________
