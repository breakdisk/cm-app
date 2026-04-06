package io.logisticos.driver.core.network.service

import okhttp3.MultipartBody
import okhttp3.RequestBody
import retrofit2.http.Multipart
import retrofit2.http.POST
import retrofit2.http.Part

interface PodApiService {
    @Multipart
    @POST("pod/submit")
    suspend fun submitPod(
        @Part("task_id") taskId: RequestBody,
        @Part photo: MultipartBody.Part?,
        @Part signature: MultipartBody.Part?,
        @Part("otp_token") otpToken: RequestBody?
    )
}
