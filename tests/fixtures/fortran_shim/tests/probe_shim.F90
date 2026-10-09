program probe_shim
  use shim_mod
  implicit none
  real(8) :: a(4), b(4), c(4), m(2,2), v(3), s
  integer :: n
  a = (/ 1.0d0, 2.0d0, 3.0d0, 4.0d0 /)
  b = (/ 10.0d0, 20.0d0, 30.0d0, 40.0d0 /)
  n = 4
  call explicit_add(a, b, c, n)
  write(*,'(4(F4.1,1X))') c
  m = reshape((/ 1.0d0, 2.0d0, 3.0d0, 4.0d0 /), (/ 2, 2 /))
  s = assumed_sum(m)
  write(*,'(F4.1)') s
  v = (/ 1.0d0, 2.0d0, 3.0d0 /)
  call fscale(v, 3, 2.0d0)
  write(*,'(3(F4.1,1X))') v
end program probe_shim
