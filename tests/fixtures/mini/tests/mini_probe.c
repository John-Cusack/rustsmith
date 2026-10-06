#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int mini_add(int a, int b);
int mini_mul(int a, int b);

int main(int argc, char **argv) {
  if (argc != 4) {
    fprintf(stderr, "usage: mini_probe add|mul A B\n");
    return 2;
  }
  int a = atoi(argv[2]);
  int b = atoi(argv[3]);
  int r;
  if (strcmp(argv[1], "add") == 0) {
    r = mini_add(a, b);
  } else if (strcmp(argv[1], "mul") == 0) {
    r = mini_mul(a, b);
  } else {
    fprintf(stderr, "unknown op\n");
    return 2;
  }
  printf("%d\n", r);
  return 0;
}
