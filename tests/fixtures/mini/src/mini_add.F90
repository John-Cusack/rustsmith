module mini_mod
  use iso_c_binding
  implicit none
contains
  function mini_add(a, b) bind(C, name="mini_add") result(c)
    integer(c_int), value :: a, b
    integer(c_int) :: c
    c = a + b
  end function mini_add
  function mini_mul(a, b) bind(C, name="mini_mul") result(c)
    integer(c_int), value :: a, b
    integer(c_int) :: c
    c = a * b
  end function mini_mul
end module mini_mod
