package io.logisticos.driver.feature.profile.data

import io.logisticos.driver.core.network.service.HomeMoveApiService
import io.logisticos.driver.core.network.service.PodApiService
import io.logisticos.driver.core.network.service.SurveyPhotoDto
import io.logisticos.driver.core.network.service.SurveyPhotoRequest
import io.logisticos.driver.core.network.service.SurveyPhotoUploadRequest
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.io.File
import java.time.Instant
import javax.inject.Inject
import javax.inject.Named

/**
 * A survey photo to the media store and onto the move: a presigned PUT from
 * pod (the same path as proof-of-pickup photos), then order-intake records it
 * against the move with its kind, room, caption and the shutter time.
 */
open class SurveyPhotoUploader @Inject constructor(
    private val podApi: PodApiService,
    private val homeApi: HomeMoveApiService,
    @Named("r2_upload") private val r2: OkHttpClient,
) {
    open suspend fun upload(
        shipmentId: String,
        file: File,
        kind: String,
        room: String?,
        caption: String,
        shutterAtMillis: Long,
    ): SurveyPhotoDto {
        val contentType = "image/jpeg"
        val slot = podApi.getSurveyPhotoUploadUrl(SurveyPhotoUploadRequest(shipmentId, contentType)).data
        withContext(Dispatchers.IO) {
            val req = Request.Builder()
                .url(slot.uploadUrl)
                .put(file.readBytes().toRequestBody(contentType.toMediaType()))
            slot.uploadHeaders.forEach { (k, v) -> req.addHeader(k, v) }
            r2.newCall(req.build()).execute().use { resp ->
                check(resp.isSuccessful) { "Photo upload failed (${resp.code})" }
            }
        }
        return homeApi.addPhoto(
            shipmentId,
            SurveyPhotoRequest(
                kind = kind,
                room = room,
                caption = caption.trim(),
                objectKey = slot.s3Key,
                contentType = contentType,
                sizeBytes = file.length(),
                // Taken at the shutter, not now: the photo's time is the
                // moment it was taken (the chain-of-custody rule).
                deviceTimestamp = Instant.ofEpochMilli(shutterAtMillis).toString(),
            ),
        ).data
    }
}
