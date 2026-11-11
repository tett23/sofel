# ADR 0038: split the generated include list without changing its order.
# The caller provides a fresh output directory with -v out=DIR.
/^#include "[^"]+\.cc"$/ && $0 !~ /^#include "\// {
    includes[NR] = $0
    next
}
{
    printf "unsupported HarfBuzz include list at line %d\n", NR > "/dev/stderr"
    invalid = 1
}
END {
    if (NR == 0) {
        print "empty HarfBuzz include list" > "/dev/stderr"
        exit 1
    }
    if (invalid) exit 1
    groups = NR < 4 ? NR : 4
    for (i = 1; i <= NR; i++) {
        unit = int((i - 1) * groups / NR)
        path = sprintf("%s/part-%02d.cc", out, unit)
        print includes[i] > path
    }
}
