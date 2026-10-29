! T026 Fortran-caller arm: an UNPORTED Fortran caller invoking the
! ported module procedure. Linked against the Rust staticlib (which exports
! `__oval_mod_MOD_oval_scale`) plus the accessor objects — WITHOUT
! oval_mod.o, so a successful link proves the Rust side owns the symbol
! (the FourierLoss -> MagnetoDynamics cross-.so pattern, in miniature).
program probe_fcall
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
end program probe_fcall
