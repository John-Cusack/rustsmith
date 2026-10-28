! T020 callback-ABI fixtures: the caller-side shapes a Rust solver-entry
! cdylib needs when IT calls into Elmer (DefUtils-style callbacks).
! Pinned to gfortran 13.3.0 (hidden lengths, result ABI, and descriptor
! layout are version-specific, ADR-027 §1/§3).
module cb_mod
  implicit none
  type vec_t
    sequence
    real(8) :: x, y
    integer :: n
  end type vec_t
  ! Single-component types: the component sits at offset 0 by
  ! construction, so the Rust side makes no layout assumption beyond that.
  type alias_t
    real(8), pointer :: p(:)
  end type alias_t
  type box_t
    real(8), allocatable :: items(:)
  end type box_t
  real(8), target, save :: alias_target(3) = [1.5d0, 2.5d0, 3.5d0]
contains
  subroutine cb_showlen(s, tag)
    character(len=*) :: s
    integer :: tag
    write(*,'(A,I0,A,I0,A,A,A)') 'TAG=', tag, ' LEN=', len(s), ' S=[', s, ']'
    flush(6)
  end subroutine cb_showlen
  function cb_makevec(a, b) result(v)
    real(8) :: a, b
    type(vec_t) :: v
    v%x = a
    v%y = b
    v%n = 42
  end function cb_makevec
  function cb_alias() result(r)
    type(alias_t) :: r
    r%p => alias_target
  end function cb_alias
  function cb_target_addr() result(a)
    use iso_c_binding
    type(c_ptr) :: a
    a = c_loc(alias_target)
  end function cb_target_addr
  subroutine cb_fillbox(b, n)
    type(box_t) :: b
    integer :: n
    integer :: i
    allocate(b%items(n))
    do i = 1, n
      b%items(i) = 10.0d0 * i
    end do
  end subroutine cb_fillbox
  subroutine cb_freebox(b)
    type(box_t) :: b
    if (allocated(b%items)) deallocate(b%items)
  end subroutine cb_freebox
end module cb_mod
