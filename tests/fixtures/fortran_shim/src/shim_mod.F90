module shim_mod
  implicit none
contains
  subroutine explicit_add(a, b, c, n)
    integer, intent(in) :: n
    real(8), intent(in) :: a(n), b(n)
    real(8), intent(out) :: c(n)
    integer :: i
    do i = 1, n
      c(i) = a(i) + b(i)
    end do
  end subroutine explicit_add
  function assumed_sum(a) result(s)
    real(8), intent(in) :: a(:,:)
    real(8) :: s
    s = sum(a)
  end function assumed_sum
end module shim_mod
