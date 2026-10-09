      subroutine cstep(x)
      implicit none
      real*8 x, cnt, acc
      common /shstate/ cnt, acc
      cnt = cnt + 1.0d0
      acc = acc + x
      x = acc
      return
      end
