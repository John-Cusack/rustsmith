      subroutine fscale(x, n, alpha)
      implicit none
      integer n, i
      real*8 x(*), alpha
      do i = 1, n
        x(i) = x(i) * alpha
      end do
      return
      end
