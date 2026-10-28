! Fortran reference driver for the T020 callback-ABI fixtures. Its stdout
! is the byte-identical reference the Rust callers must reproduce.
program probe_cb
  use cb_mod
  implicit none
  type(vec_t) :: v
  type(alias_t) :: r
  type(box_t) :: b
  call cb_showlen('Source', 1)
  call cb_showlen('BoundaryOnly', 2)
  v = cb_makevec(3.0d0, 4.0d0)
  write(*,'(A,F4.1,A,F4.1,A,I0)') 'VEC x=', v%x, ' y=', v%y, ' n=', v%n
  flush(6)
  r = cb_alias()
  if (associated(r%p, alias_target)) then
    write(*,'(A,F3.1,A,F3.1,A,F3.1,A)') &
      'ALIAS-R=', r%p(1), ',', r%p(2), ',', r%p(3), ' FLAG=ALIAS'
  else
    write(*,'(A)') 'ALIAS-R=? FLAG=COPY'
  end if
  flush(6)
  call cb_fillbox(b, 4)
  write(*,'(A,F4.1,A,F4.1,A,F4.1,A,F4.1)') &
    'BOX=', b%items(1), ',', b%items(2), ',', b%items(3), ',', b%items(4)
  flush(6)
  call cb_freebox(b)
end program probe_cb
