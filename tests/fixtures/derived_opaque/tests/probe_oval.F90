! T026 reference driver: plain-Fortran use of the admitted shape.
! `pos` (Rust caller) and `fcall` (Fortran caller into the Rust proc) must
! both print byte-identical output to this reference.
program probe_oval
  use oval_mod
  implicit none
  type(oval_t) :: o
  o%a = 1.5_dp
  o%n = 3
  allocate(o%w(3))
  o%w = 1.5_dp
  call oval_scale(o, 2.0_dp)
  write(*,'(A,F6.2,A,I0)') 'A=', o%a, ' N=', o%n
  write(*,'(A,3F6.2)') 'W=', o%w
end program probe_oval
