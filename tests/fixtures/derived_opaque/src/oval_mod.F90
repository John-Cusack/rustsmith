! T026 derived-opaque fixtures: the solver-idiom module whose scalar
! derived-type dummy crosses the plugin boundary as an opaque address.
! `oval_scale` is the ADMITTED shape (scalar TYPE + scalar REAL, both plain);
! `oval_fill` (allocatable array), `oval_make` (derived RESULT), and
! `oval_use_ptr` (pointer dummy) are the REFUSED shapes — the gate still
! refuses them, and the proof script forces each through anyway to show the
! failure is observable (never silently green).
module oval_mod
  use iso_c_binding
  implicit none
  integer, parameter :: dp = kind(1.0d0)
  type :: oval_t
    real(dp) :: a = 0.0_dp
    integer :: n = 0
    real(dp), allocatable :: w(:)
  end type oval_t
contains
  subroutine oval_scale(oval, factor)
    type(oval_t), intent(inout) :: oval
    real(dp), intent(in) :: factor
    integer :: i
    oval%a = oval%a * factor
    if (allocated(oval%w)) then
      do i = 1, size(oval%w)
        oval%w(i) = oval%w(i) * factor
      end do
    end if
  end subroutine oval_scale
  subroutine oval_fill(list, n)
    type(oval_t), allocatable, intent(out) :: list(:)
    integer, intent(in) :: n
    integer :: i
    allocate(list(n))
    do i = 1, n
      list(i)%a = 1.0_dp * i
      list(i)%n = i
    end do
  end subroutine oval_fill
  function oval_make(n) result(r)
    integer, intent(in) :: n
    type(oval_t) :: r
    r%a = 2.0_dp * n
    r%n = n
  end function oval_make
  subroutine oval_use_ptr(p, factor)
    type(oval_t), pointer, intent(inout) :: p
    real(dp), intent(in) :: factor
    p%a = p%a * factor
  end subroutine oval_use_ptr
  ! Constructor/destructor behind BIND(C): the opaque address originates on
  ! the Fortran side in every arm (the Rust side never lays the object out).
  function oval_new_c(n, a0) result(ptr) bind(c, name="oval_new_c")
    integer(c_int), value :: n
    real(c_double), value :: a0
    type(c_ptr) :: ptr
    type(oval_t), pointer :: o
    allocate(o)
    o%a = a0
    o%n = n
    allocate(o%w(n))
    o%w = a0
    ptr = c_loc(o)
  end function oval_new_c
  subroutine oval_free_c(ptr) bind(c, name="oval_free_c")
    type(c_ptr), value :: ptr
    type(oval_t), pointer :: o
    call c_f_pointer(ptr, o)
    if (allocated(o%w)) deallocate(o%w)
    deallocate(o)
  end subroutine oval_free_c
end module oval_mod
