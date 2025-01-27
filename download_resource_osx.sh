#!/bin/sh

VER=20240501
FILENAME=20240501.tar.gz
DIR=Forg.app/Contents/Resources/icons/

curl -OL "https://github.com/PapirusDevelopmentTeam/papirus-icon-theme/archive/refs/tags/${FILENAME}"
tar xf "${FILENAME}" -C "${DIR}"

pushd "${DIR}" > /dev/null

PREFIX="papirus-icon-theme-${VER}"
for d in ${PREFIX}/Papirus/*/; do
    target="Papirus/`basename $d`/"
    mkdir -p ${target}/places
    cp -a "$d/mimetypes" ${target}
    cp $d/places/folder.svg ${target}/places/
done

rreadlink() {
    f=`readlink $1`
    if [[ -h $f ]]; then
	rreadlink $f
    else
	echo $f
    fi
}

for d in Papirus/*/mimetypes; do
    for f in $d/*; do
	if [[ -h $f ]]; then
	    real="$d/`rreadlink $f`"
	    if [[ ! -f ${real} ]]; then
		echo "bad link $f -> ${real}"
		mkdir -p `dirname ${real}`
		cp "${PREFIX}/${real}" "${real}"
	    fi
	fi
    done
done


popd > /dev/null

rm -rf "${DIR}/${PREFIX}"
rm "${FILENAME}"
