#include "../src/iso_writer.h"

int main(int argc, char **argv) {
  if (argc != 4) return 2;
  return dvda_iso_write(argv[1], argv[2], argv[3]) == 0 ? 0 : 1;
}
