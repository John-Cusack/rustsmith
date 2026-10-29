! T026 derived-opaque fixtures: generated-style BIND(C) accessors for
! `oval_t`, compiled against the built .mod files of the target configure —
! the accessor contract. The Rust port touches components only through
! these; it never assumes a layout.
module oval_accessor
  use iso_c_binding
  use oval_mod
  implicit none
contains
  subroutine oval_get_a(oval, out) bind(c, name="oval_get_a")
    type(c_ptr), value :: oval
    real(c_double), intent(out) :: out
    type(oval_t), pointer :: o
    call c_f_pointer(oval, o)
    out = o%a
  end subroutine oval_get_a
  subroutine oval_set_a(oval, val) bind(c, name="oval_set_a")
    type(c_ptr), value :: oval
    real(c_double), value :: val
    type(oval_t), pointer :: o
    call c_f_pointer(oval, o)
    o%a = val
  end subroutine oval_set_a
  function oval_get_n(oval) result(n) bind(c, name="oval_get_n")
    type(c_ptr), value :: oval
    integer(c_int) :: n
    type(oval_t), pointer :: o
    call c_f_pointer(oval, o)
    n = o%n
  end function oval_get_n
  subroutine oval_get_w(oval, i, out) bind(c, name="oval_get_w")
    type(c_ptr), value :: oval
    integer(c_int), value :: i
    real(c_double), intent(out) :: out
    type(oval_t), pointer :: o
    call c_f_pointer(oval, o)
    out = o%w(i)
  end subroutine oval_get_w
  subroutine oval_set_w(oval, i, val) bind(c, name="oval_set_w")
    type(c_ptr), value :: oval
    integer(c_int), value :: i
    real(c_double), value :: val
    type(oval_t), pointer :: o
    call c_f_pointer(oval, o)
    o%w(i) = val
  end subroutine oval_set_w
end module oval_accessor
